# PostgreSQL 19 raw parser profile

This profile compares the release binaries at `5809051` on an AMD Threadripper 2950X, with rustc 1.100.0 and GCC 14.2 at `-O3`. Each call builds and destroys an independently owned result. The corpus contains all 54,266 successful default-mode regression inputs. All three workloads use the `parse` mode of the [comparison benchmark](../README.md#compare-native-parser-performance-with-c).

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

For 32 PostgreSQL nonterminals, the parser can reach at least 24 distinct combinations of destination state and subsequent semantic actions. The generator now stores those combinations in tables indexed by predecessor state. In seven paired runs on CPU 4, the prototype reduced median L2 instruction-cache fills per query from 1,569 to 1,170 on the join and from 609 to 515 on the corpus. In a separate seven-round run on CPU 3, sharing the stack-handling code for all explicit reductions instead increased median paired time by 12% on the join and 8% on the corpus.

The release build matched all 54,471 reference outcomes and all 72 additional boundary cases. Its seven-round comparison on CPU 3 verified every benchmark input's JSON bytes before timing. The table lookup reduced the executable's machine code by 112,928 bytes and increased read-only data by 701,848 bytes relative to `916e8a3`.

| Workload | C parse and free, µs | Rust parse and free, µs | Rust/C |
| --- | ---: | ---: | ---: |
| `SELECT 1` | 0.472 | 0.365 | 0.774 |
| Aggregate join | 6.039 | 6.790 | 1.124 |
| Regression corpus | 1.918 | 2.442 | 1.274 |

Repeating the arena experiment with the lookup tables gave median bulk-release times of 5.945 µs on the join and 2.195 µs on the corpus against C's 6.220 and 2.122 µs on CPU 3. Grouping the inputs by their leading keyword on CPU 26 put the largest remaining bulk-release deficit under CREATE, at 2.277 µs against 2.124 µs over 9,714 inputs. Grouping changes the input order and these figures measure the experimental ownership model.

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
