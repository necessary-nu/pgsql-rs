# PostgreSQL 19 raw parser profile

## Release results

Each call builds and frees an independently owned raw tree. The table gives median
microseconds per query over seven paired rounds on each core of an AMD Threadripper
2950X. Rust 1.100.0 used a profile trained on the 15 checked-in deparser SQL files,
`panic=abort`, and statically linked mimalloc 3.3.2. PostgreSQL 19.0 used GCC 14.2
at `-O3` with glibc. None of the training inputs matched a timed input after
normalizing case, whitespace and trailing semicolons. [Reproduce the comparison](../README.md#compare-native-parser-performance-with-c).

| Workload | Rust, CPU 5 | C, CPU 5 | Rust, CPU 12 | C, CPU 12 |
| --- | ---: | ---: | ---: | ---: |
| `SELECT 1` | 0.219 | 0.415 | 0.222 | 0.423 |
| Parameterized aggregate join | 4.311 | 5.825 | 4.301 | 5.890 |
| 54,266 regression queries | 1.619 | 1.812 | 1.616 | 1.826 |

Every benchmark input produced byte-identical JSON. A probe linked with the
release's parser and allocator matched all 54,471 reference outcomes across six
parse modes and 72 additional boundary cases. A separately compiled application
using the public API also matched every outcome and beat C on all three workloads
on CPUs 7 and 14. The ordinary Cargo release with the system allocator measured
0.313, 6.447 and 2.365 µs against C's 0.433, 6.097 and 1.865 µs on CPU 6.

On CPU 5, giving C mimalloc as well brought its corpus median to 1.807 µs,
against Rust's 1.650 µs. With the C backend also profile-trained on the same
15 files, Rust and C measured 0.217 and 0.380 µs on the literal, 4.231 and
4.945 µs on the join, and 1.602 and 1.639 µs on the corpus. Repeating the
comparison on CPU 12 gave 1.602 and 1.632 µs on the corpus. On that core
with C trained on a separate 12,396-query extension workload and its usual
system allocator, corpus medians were within 0.5% (1.603 µs Rust, 1.611 µs C).

### Where the release spends its cycles

The table gives the percentage of user-mode cycle samples on CPU 3 for Rust
and C respectively. Each phase includes its allocator calls. The remainder
covers the benchmark loop and samples outside those phases.

| Workload | Scanner, Rust / C | Parser and tree construction, Rust / C | Result cleanup, Rust / C |
| --- | ---: | ---: | ---: |
| `SELECT 1` | 4.2 / 26.5 | 79.5 / 67.6 | 12.2 / 5.1 |
| Parameterized aggregate join | 15.0 / 38.1 | 73.4 / 61.5 | 11.2 / 0.3 |
| Regression corpus | 19.1 / 41.1 | 68.6 / 57.2 | 11.0 / 1.4 |

On the corpus the inlined Rust parser driver accounted for 31.0% of samples
in its own instructions. Identifier scanning took 7.3%, the general scanner
path took 6.0% and `mi_free` took 5.4%. PostgreSQL's `base_yyparse` and
`core_yylex` accounted for 46.7% and 25.5% respectively.

## Initial release at `5809051`

The following measurements used the same processor, compilers and workloads.

## Where the instructions go

Callgrind counted the timed loop after warmup. Each cell is instructions per input, rounded to the nearest integer. The scanner column includes setup and lookahead. The parser column includes grammar decisions, semantic actions, tree allocation, and C memory-context creation. Cleanup starts when the caller releases the returned tree. The remainder is the benchmark loop, including inlined caller bookkeeping.

| Workload | Implementation | Scanner | Parser and tree construction | Result cleanup | Remainder | Total |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `SELECT 1` | C | 1,234 | 2,762 | 148 | 27 | 4,171 |
| | Rust | 678 | 3,503 | 653 | 56 | 4,890 |
| Aggregate join | C | 17,734 | 25,123 | 148 | 27 | 43,032 |
| | Rust | 17,337 | 39,526 | 11,824 | 56 | 68,743 |
| Regression corpus | C | 5,156 | 6,548 | 150 | 25 | 11,879 |
| | Rust | 5,131 | 10,149 | 2,429 | 56 | 17,765 |

Rust executes 5,886 extra instructions per corpus query. Grammar processing and tree construction account for 3,602 of them (61%), and releasing the returned tree accounts for 2,279 (39%). On the join the corresponding differences are 14,403 and 11,676. The scanner saves 397 instructions on the join and 556 on `SELECT 1`.

Hardware counters independently measured median instructions per query over five rounds. In workload order, C retired 4,115, 42,861 and 11,925 instructions. Rust retired 4,886, 68,745 and 17,783. Hardware counts include process startup and warmup amortized across the calls. Callgrind counts the timed loop and models instructions differently.

## Where the cycles go

Linux perf sampled user-mode cycles with call stacks on logical CPU 5. Each percentage uses that workload and implementation's own sample count as its denominator. The three Rust columns are disjoint. The driver column measures samples in the driver itself, including inlined work. The handlers column includes everything called by a generated reduction handler. The cleanup columns count recognizable result destruction in the caller. Scanner work, caller bookkeeping and other work account for the remaining samples.

| Workload | Rust driver | Rust reduction handlers | Rust result cleanup | C result cleanup | Samples, Rust / C |
| --- | ---: | ---: | ---: | ---: | ---: |
| `SELECT 1` | 21.5% | 49.0% | 11.2% | 5.0% | 14,965 / 12,923 |
| Aggregate join | 22.2% | 41.0% | 13.8% | 0.3% | 16,181 / 11,522 |
| Regression corpus | 20.4% | 40.0% | 13.4% | 1.1% | 18,630 / 11,633 |

### Reduction and allocation sites

* On `SELECT 1`, Rust enters 13 reduction handlers. The `simple_select` handler uses 965 instructions, building the target element uses 398, and reducing `simple_select` to `select_no_parens` uses 334. These counts include folded actions called by the handler. Seven empty optional clauses together use 306 instructions. DHAT attributes the six heap blocks to boxing the target value, its `ResTarget` record, its one-element target list, its `SelectStmt` record, the statement payload and the result vector.
* On the join, Rust enters 103 handlers. Seven qualified column references spend 5,350 instructions in `columnref: ColId indirection` and 2,184 in `indirection_el: '.' attr_name`. Together these two handlers account for 30% of the 25,460 instructions spent under reduction handlers. DHAT attributes 21 heap blocks per query to scanning tokens, 16 to operator construction and 14 to forming the first indirection element's list. [Column and operator construction](../../../crates/pg19-parser/src/expr.rs)
* On the corpus, Rust enters 28.7 handlers per input on average. The biggest individual handler is `simple_select` at 473 instructions per input. Other recurring costs include attaching a statement to its list (236), forming a qualified name from a column identifier (233), and building qualified and unqualified column references (209 and 208). The parser driver itself uses 3,647 instructions per input. [Parser driver](../../../crates/pg19-engine/src/lib.rs) · [Statement construction](../../../crates/pg19-parser/src/query.rs)

The following counts come from interposing glibc `malloc`, `calloc`, `realloc` and `free` on the unmodified executables. Subtracting two run lengths removes fixed startup costs. For the corpus the difference covers 11 complete passes including warmup. PostgreSQL serves node allocations from its memory context and reuses blocks across calls.

| Workload | Rust malloc/calloc | Rust realloc | Rust non-null free | C malloc/calloc | C non-null free |
| --- | ---: | ---: | ---: | ---: | ---: |
| `SELECT 1` | 6 | 0 | 6 | 0 | 0 |
| Aggregate join | 90 | 8 | 90 | 0 | 0 |
| Regression corpus | 20.38 | 0.96 | 20.38 | 0.009 | 0.009 |

## After folding omitted grammar clauses

At `02e4aa8`, the raw parser matched all 54,471 reference outcomes. The seven-round comparison on logical CPU 17 verified identical JSON bytes for every benchmark input before timing. Times below are median microseconds per query, including destruction. Callgrind counted instructions on the same Rust release binary in separate runs.

| Workload | C µs | Rust µs | Rust/C | Rust instructions | In reduction handlers | Releasing the result |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `SELECT 1` | 0.430 | 0.350 | 0.814 | 3,430 | 1,264 | 653 |
| Aggregate join | 6.038 | 6.979 | 1.156 | 57,252 | 20,174 | 11,117 |
| Regression corpus | 1.888 | 2.722 | 1.442 | 15,506 | 5,642 | 2,408 |

Folding the omitted SELECT and aggregate clauses reduced handler work on `SELECT 1` from 2,161 instructions at `8e795a4` to 1,264. The driver now includes common scanner paths and uses 1,091 instructions on `SELECT 1`, 18,483 on the join and 4,852 per corpus input.

Releasing the join tree costs Rust 10,969 more instructions than C, or 77% of their total instruction difference. On the corpus the corresponding figures are 2,258 instructions and 62%. On the join, Rust makes 90 allocations, one reallocation and 90 frees per query. On the corpus it averages 20.35 allocations, 0.71 reallocations and 20.35 frees. The C memory context needs zero system allocator calls on the join and averages 0.009 allocations and frees per corpus input after warmup.

Hardware cycle sampling on logical CPU 20 gives a different split. The cleanup estimates multiply the fraction of samples in result destruction by the median hardware cycle count per query. Driver self-time includes inlined scanning. Each of these six profiles had 10,342–17,425 samples.

| Workload | C cycles | Rust cycles | C cleanup cycles | Rust cleanup cycles | Rust driver | Rust reduction handlers |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `SELECT 1` | 1,781 | 1,403 | 88 | 221 | 32.6% | 35.6% |
| Aggregate join | 24,261 | 28,012 | 84 | 4,438 | 32.3% | 39.3% |
| Regression corpus | 7,602 | 10,692 | 89 | 1,495 | 27.6% | 41.1% |

The extra 4,354 cycles spent releasing the join tree account for the entire measured gap. Before cleanup Rust saves about 600 cycles on that input. For the corpus, cleanup accounts for about 1,406 of the 3,090 excess cycles per query (46%). Scanning, building the tree and caller bookkeeping account for the other 1,684.

In a separate timing run, grouping the corpus by leading SQL keyword put 18,934 inputs under SELECT and 9,714 under CREATE. Together they accounted for 22.1 ms of the 35.2 ms Rust–C difference across groups, weighted by input count. Each group had seven paired rounds pinned to CPU 6. Grouping changes the order in which the parser sees statements, so the full-corpus row above remains the latency comparison.

## Finishing a statement list

Of the 54,266 corpus inputs, 54,013 end with a semicolon. When the grammar tables prove that only the empty trailing statement and end-of-input reductions remain, Rust can set the preceding statement's byte length and finish the list. All 54,471 reference outcomes matched, as did 72 additional cases covering omitted clauses, errors and the stack limit.

In seven paired raw runs on CPU 2, the new release binary retired 3,343 instructions on `SELECT 1`, 57,243 on the join and 15,057 per corpus input. The previous release binary retired 3,429, 57,270 and 15,560 respectively. Corpus time fell in all seven pairs, with a median reduction of 7.7%. The join's median paired time rose 1.2%, with individual times varying from 7.6 to 16.2 µs across both binaries.

The separate seven-round comparison with optimized C on CPU 31 verified identical JSON bytes for every input. Median Rust and C parse-and-free times were 0.663 and 0.858 µs for `SELECT 1`, 13.058 and 10.275 µs for the join, and 5.281 and 3.421 µs per corpus input. The join and corpus still exceed the C target.

### Instruction fetch and tree ownership

At `916e8a3`, seven hardware-counter runs on CPU 2 measured the following medians per query. On this processor, `ic_cache_fill_l2` counts 64-byte instruction-cache lines supplied by L2, and `ic_fetch_stall.ic_stall_dq_empty` counts cycles when the instruction pipe reports an empty-queue stall.

| Workload | C µs | Rust µs | L2 instruction fills, C / Rust | Empty-queue stall cycles, C / Rust |
| --- | ---: | ---: | ---: | ---: |
| `SELECT 1` | 0.438 | 0.363 | 0.3 / 5.8 | 27 / 18 |
| Aggregate join | 6.147 | 7.314 | 41 / 1,638 | 571 / 841 |
| Regression corpus | 1.897 | 2.673 | 20 / 604 | 169 / 645 |

In separate samples of the L2 instruction-fill event on CPU 24, 38% of Rust's join samples and 37% of its corpus samples occurred under a generated reduction handler. Releasing the returned tree accounted for 18% and 19% respectively. A fill can overlap useful execution, so these percentages describe cache traffic.

A single-threaded experiment kept each complete Rust tree in a per-query arena and compared recursive destruction with reclaiming the whole arena at once. All three allocator modes produced the reference JSON for every benchmark input. With recursive destruction, the allocation audit found zero outstanding allocations after each result. The experiment reserved the parser stack before measurement and confined all returned nodes to the arena's lifetime.

Across seven paired rounds on CPU 12, arena allocation with recursive destruction reduced elapsed time against the experiment's system-allocator control by 19% on the join and 14% on the corpus. Bulk release increased those reductions to 25% and 23%. Even with bulk release the corpus took a median 3.920 µs against C's 3.442 µs. The public parser returns independently owned nodes and its benchmark includes their ordinary destruction.

## Looking up grammar destinations

For 32 PostgreSQL nonterminals, the parser can reach at least 24 distinct combinations of destination state and subsequent semantic actions. The generator now stores those combinations in tables indexed by predecessor state. In seven paired runs on CPU 7, `d7bedb4` reduced median L2 instruction-cache fills per query from 1,559 to 1,279 on the join and from 613 to 513 on the corpus relative to `916e8a3`. A separate seven-round comparison of the same release binaries on CPU 10 measured median paired time reductions of 5.2% on the join and 6.1% on the corpus.

Two other changes reduced code size or instruction counts at a cost in elapsed time. Sharing the stack-handling code for all explicit reductions increased median paired time by 12% on the join and 8% on the corpus on CPU 3. Combining omitted clauses in the INSERT, CREATE TABLE and column-definition actions saved about 100 instructions per corpus input but increased median paired time by 2.6%.

The release build matched all 54,471 reference outcomes and all 72 additional boundary cases. Its seven-round comparison on CPU 3 verified every benchmark input's JSON bytes before timing. The table lookup reduced the executable's machine code by 112,928 bytes and increased read-only data by 701,848 bytes relative to `916e8a3`.

| Workload | C parse and free, µs | Rust parse and free, µs | Rust/C |
| --- | ---: | ---: | ---: |
| `SELECT 1` | 0.472 | 0.365 | 0.774 |
| Aggregate join | 6.039 | 6.790 | 1.124 |
| Regression corpus | 1.918 | 2.442 | 1.274 |

Repeating the arena experiment with the lookup tables gave median bulk-release times of 5.945 µs on the join and 2.195 µs on the corpus against C's 6.220 and 2.122 µs on CPU 3. Grouping the inputs by their leading keyword on CPU 26 put the largest remaining bulk-release deficit under CREATE, at 2.277 µs against 2.124 µs over 9,714 inputs. Grouping changes the input order and these figures measure the experimental ownership model.

## Raw parse costs with the grammar lookup tables

These cycle samples come from the same two release executables used for the
`d7bedb4` comparison. Linux perf sampled user-mode cycles on CPU 6. Each
row gives percentages of that executable's samples. The parser column includes
building the tree and code inlined into the driver. Cleanup starts in the
caller after the parser returns.

| Workload | Implementation | Scanner | Parser and construction | Result cleanup | Other | Samples |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `SELECT 1` | Rust | 4.6% | 76.8% | 15.4% | 3.2% | 11,057 |
| | C | 25.7% | 68.6% | 5.0% | 0.7% | 13,757 |
| Aggregate join | Rust | 12.4% | 69.4% | 18.1% | 0.2% | 12,787 |
| | C | 38.6% | 60.9% | 0.3% | 0.1% | 11,593 |
| Regression corpus | Rust | 16.2% | 67.9% | 15.1% | 0.8% | 12,872 |
| | C | 42.4% | 56.1% | 1.2% | 0.3% | 10,119 |

Storing short strings inline reduced heap allocations on the join from 90
to 65 and on the corpus from 20.31 to 16.06 per input. The complete trees
produced identical JSON bytes. In seven paired raw runs on CPU 21, the join
took 3.2% longer and the corpus took 2.4% longer. Storing just the
`ColumnRef` record inline saved seven allocations on the join and sped it up
1.9%, but corpus time increased 3.7%. Caching subtree depth for destruction
increased join and corpus times by 5.8% and 5.6% on CPU 10.

### Training the release compiler on other SQL

Across seven paired runs, a Rust build trained on 263 separate SQL fixtures
cut raw join time by 16.7% and corpus time by 13.5% relative to the ordinary
Rust release. Rust and optimized C medians were 0.311 and 0.423 µs for
`SELECT 1`, 5.551 and 6.057 µs for the join, and 2.113 and 1.938 µs for the
corpus. The heap allocation counts in the timed code were independent of
compiler training. Callgrind counted 51,204 instructions per join in the
profiled build and 56,541 in the ordinary build.

A separate comparison trained both compilers on the same 12,396 PostgreSQL
extension queries, with four passes each through raw parse and parse plus
JSON. The training inputs had zero matches to the timing inputs after
normalizing case, whitespace and trailing semicolons. In seven paired raw
rounds the following medians included freeing the result.

| Workload | Rust with profile, µs | GCC `-O3`, µs | GCC `-O3` with profile, µs |
| --- | ---: | ---: | ---: |
| `SELECT 1` | 0.320 | 0.428 | 0.394 |
| Aggregate join | 5.737 | 6.015 | 5.075 |
| Regression corpus | 2.126 | 1.865 | 1.703 |

The profiled Rust build beat both C builds on the literal query. It beat
unprofiled C on the join and exceeded both C medians on the corpus.

The [repository build example](../README.md#build-with-a-parser-profile)
uses 15 checked-in SQL files so it can run from a fresh checkout. That
release matched all 54,471 reference outcomes and 72 additional boundary
cases. Before timing, the benchmark verified identical JSON bytes for every
input. On CPU 17, its seven-round raw Rust/C medians were 0.316/0.428 µs,
5.539/6.090 µs and 2.309/1.890 µs. The full corpus remains 22.2% over
the C target with that training set.

## Discarding semantic values on the parser stack

The parser now groups the values that own heap storage under one private
enum variant. Discarding punctuation or a value already moved into the
result takes one discriminant check. Seven paired runs on CPU 2 compared
the release executables before and after the change. Median paired
parse-and-free time fell by 9.3% on `SELECT 1`, 3.9% on the join and 2.7%
on the corpus. Retired instructions per input fell from 3,309 to 3,101,
56,546 to 54,934 and 14,944 to 14,534 respectively.

The ordinary release and the release trained on 263 other SQL fixtures each
matched all 54,471 reference outcomes and all 72 additional boundary cases.
Each seven-round benchmark also verified identical JSON bytes for every
input before timing. Median raw parse-and-free times were as follows.

| Workload | Ordinary Rust, µs | Paired C, µs | Profiled Rust, µs | Paired C, µs |
| --- | ---: | ---: | ---: | ---: |
| `SELECT 1` | 0.318 | 0.432 | 0.305 | 0.442 |
| Aggregate join | 6.600 | 6.078 | 5.653 | 6.197 |
| Regression corpus | 2.471 | 1.889 | 2.197 | 1.917 |

A separate seven-round comparison on CPU 4 ran the profiled Rust and C
executables with the same allocator. With glibc, median corpus time was
2.151 µs for Rust and 1.955 µs for C. With mimalloc v3, those times were
1.994 and 1.940 µs. The remaining corpus gap was 2.8% with that allocator.

At `e8d58e0`, cycle sampling on CPU 6 with the same profiled Rust and
mimalloc builds gave the following split. Each pair gives the Rust and C
percentages of that executable's own samples. Other code accounts for the
remainder.

| Workload | Scanner, Rust / C | Parser and construction, Rust / C | Result cleanup, Rust / C |
| --- | ---: | ---: | ---: |
| `SELECT 1` | 3.8 / 27.0 | 76.6 / 68.0 | 11.9 / 4.6 |
| Aggregate join | 10.9 / 37.6 | 76.7 / 61.9 | 11.5 / 0.4 |
| Regression corpus | 17.1 / 42.6 | 71.1 / 55.8 | 10.0 / 1.1 |

Of the samples in the Rust driver itself, 10.9%, 22.7% and 13.0% respectively
landed on the same instruction in the sequence that writes a 56-byte shifted
frame. Checking the lookahead in Bison's action table accounted for another 9.8% of
driver samples on the corpus. Cycle sampling can attribute stalls to a
nearby instruction.

With mimalloc 3.3.2 built using initial-exec TLS and its optional AVX2
configuration for both languages, and LLVM's extended TSP block placement
enabled on the profiled Rust release, two independent seven-round comparisons
still missed corpus parity. Rust and C took 1.919 and 1.907 µs on CPU 4 and
2.024 and 1.918 µs on CPU 10. The release matched all 54,471 reference
outcomes and all 72 boundary cases.

In seven paired runs on CPU 13, writing a shifted value directly into its
stack slot reduced retired instructions per corpus input by 3.4%, but median
paired parse-and-free time rose 0.7%. Choosing the slot's variant before
the write saved 3.9% of instructions and raised elapsed time 2.2%.

### Matching a profile to a consuming application

A separate application used the public `pg19-query` dependency and the same
parser source as the benchmark. With the repository's profile,
`llvm-profdata show --function` found no entry for the application's
mangled `pg19_parser::parse_statements` symbol. Training on the same 263
fixtures using the application's Cargo manifest produced a profile with
counts for that exact symbol. The training helper accepts the manifest as
its optional third argument.

### Panic strategy and raw parse time

At `a15f14c`, a Rust release trained on 263 separate SQL fixtures with
`CARGO_PROFILE_RELEASE_PANIC=abort` passed the full comparison on two
physical cores. Both programs used the same mimalloc 3.3.2 shared library.
Each run verified every JSON tree before seven paired timing rounds. Times
include destroying the result and are median microseconds per query.

| Workload | Rust, CPU 5 | C, CPU 5 | Rust, CPU 12 | C, CPU 12 |
| --- | ---: | ---: | ---: | ---: |
| `SELECT 1` | 0.232 | 0.451 | 0.234 | 0.445 |
| Aggregate join | 4.473 | 6.199 | 4.413 | 6.162 |
| Regression corpus | 1.767 | 1.928 | 1.818 | 1.891 |

The release matched all 54,471 reference outcomes and 72 boundary cases.
On CPU 5 with glibc, corpus time was 1.910 µs for Rust and 1.847 µs for C.
On CPU 12 with mimalloc and additional LLVM BOLT optimization of the C
binary on the same training inputs, corpus time was 1.815 µs for Rust and
1.854 µs for C.

### Identifier scanning

The scanner writes keywords and identifiers into the caller's token slot
and limits unquoted identifiers to PostgreSQL's 63-byte UTF-8 boundary
before allocating their lowercase text. In seven paired runs on CPU 23,
the ordinary release cut median paired raw time by 1.3% on `SELECT 1`,
3.7% on the join and 1.9% across the corpus. Corpus instructions per input
fell from 14,534 to 14,245.

The core and grammar scanners produced identical token streams on 97,356
cases with each backslash-quote setting tested. The release also matched
all 54,471 reference outcomes and 72 boundary cases. With the ordinary
release settings on CPU 1, Rust took 0.310, 6.380 and 2.337 µs on the
three workloads against C's 0.429, 6.095 and 1.862 µs.

## Reproducing the profiles

Use a benchmark output directory containing `bench-c`, `bench-rust`, and the three `.sql0` inputs. This example profiles the join. For the full set, the perf repeat counts were 13,000,000 for `simple`, 850,000 for `join`, and 52 for `corpus`. The Callgrind repeat counts were 10,000, 1,000 and 1 respectively.

```sh
cd "$benchmark_output"
workload=join
binary=bench-rust
loop='parse_bench::run'
cpu=5
taskset -c "$cpu" perf record --call-graph dwarf,4096 -e cycles:u -c 2000003 \
    -o "$workload-$binary.data" -- "./$binary" parse "$workload.sql0" 850000
perf report -i "$workload-$binary.data" --stdio
valgrind --tool=callgrind --collect-atstart=no "--toggle-collect=$loop" \
    "--dump-after=$loop" "--callgrind-out-file=$workload-$binary.cg" \
    "./$binary" parse "$workload.sql0" 1000
callgrind_annotate --inclusive=yes --tree=calling "$workload-$binary.cg.2"
```

Set `binary=bench-c` and `loop=run` for C. Dump 1 is the warmup and dump 2 is the timed loop. For allocation stacks, run the same executable and input with `valgrind --tool=dhat --num-callers=32`.
