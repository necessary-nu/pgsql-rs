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
