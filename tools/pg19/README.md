# Maintaining the PostgreSQL 19 grammar

Run this command from the repository root to build both C references and
check the raw-parser corpus.

```sh
cargo run -p pg19-reference -- import /tmp/pg19-runs
```

The importer takes a work directory and an optional path to a local
`postgresql-19.0.tar.bz2`. It verifies the pinned SHA-256 for both downloaded
and local archives. Each invocation creates a fresh run directory and prints
its path. Allow space for two source and build trees. The build uses Linux,
a C toolchain with GNU-compatible linker wrapping, GNU make, Flex, Bison,
Perl, tar with bzip2, GNU patch, sha256sum, and curl for downloads.
`PG19_JOBS` controls the number of build jobs per backend. The initial
snapshots were verified with GCC 14.2, Bison 3.8.2 and Flex 2.6.4.

The run directory holds the original archive, `stock-source`, `patched-source`,
and the `stock/probe` and `patched/probe` executables. Both source trees
retain `gram.y`, `scan.l`, the parser lookahead code, and node and keyword
headers. The patched tree uses the [PG19 compatibility series](patches/README.md).
The C library at the repository root continues to build against its 18.6 snapshot.

### How upstream changes reach this repository

The original C workflow updates PostgreSQL through maintainer commits. Its
Makefile pins both the archive version and the numeric release. The
`tmp/postgres` target downloads that release into a fixed directory, applies
14 project patches, configures PostgreSQL and invokes its generators.
`make extract_source` replaces `src/postgres` with the C dependency closure
selected by the Ruby/libclang extractor and copies the upstream regression SQL.
The recipe assumes Apple's libclang location and BSD `sed`. C builds and CI
consume the committed snapshot. The repository history shows a major upgrade
in `bd17fe2` and a point release in `e087c65`. [Legacy extraction recipe](../../Makefile)

For the Rust port, the release archive and the reviewed patch series are the
source of truth. The scanner imports Flex's transitions and the keyword
catalog. The parser imports Bison's resolved tables and inventories each
production and semantic action. The raw node generator reads the patched
headers with Clang. The 19.0 Rust parser implements all 2,326 explicit
grammar actions and uses Bison's default reductions for the remaining rules.
A release update therefore has reviewable diffs for
syntax, actions and data layout. The stock and patched probes establish
whether a behavioral change came from PostgreSQL or from the overlay.
[Parser table regeneration](../../crates/pg19-tables/README.md)

### Regenerating native record and utility actions

After refreshing the Bison inventory and `simple.tsv`, run the typed
constructor translator against the same patched build. It needs Python 3
and `rustfmt`.

```sh
python3 tools/pg19/generate-record-actions.py "$run/patched"
python3 tools/pg19/generate-record-actions.py "$run/patched" --check
PG19_REFERENCE_DIR="$run" cargo test -p pg19-parser --test utility -- --include-ignored
```

The translator matches a complete C reduction against recognized allocation,
field assignment, location, list and helper-call forms. It uses the generated
Rust node definitions for the field types. It emits `records.tsv` and
`src/records.in` in `crates/pg19-parser`. Handwritten family actions override
generated fallbacks. A new or changed action requires a reviewed mapping
before the complete-inventory test passes. The corpus comparison identifies
SQL inputs that reach an unmapped reduction.

### Comparing parser behavior

Set `run` to the path printed by the importer to inspect a default-mode SQL
input or recheck an existing build.

```sh
"$run/stock/probe" 'SET ROLE $1;'
"$run/patched/probe" 'SET ROLE $1;'
"$run/patched/probe" 'character varying(32)' --mode type-name
"$run/stock/probe" 'v.x := 1' --mode plpgsql-assign2 --backslash-quote off
cargo run -p pg19-reference -- compare "$run"
```

Pass the SQL as the first argument, including when it starts with a dash.
The optional `--mode` selects `default`, `type-name`, `plpgsql-expr`,
`plpgsql-assign1`, `plpgsql-assign2` or `plpgsql-assign3`. Assignment
modes specify how many dotted name components form the target. The optional
`--backslash-quote` selects `off`, `on` or `safe_encoding` (also `safe`) for
both parsing and scanning. Each invocation runs in its own process. The
defaults are ordinary SQL mode and `safe_encoding` with UTF-8 input.

Each probe emits JSON. The tree is PostgreSQL's raw node serialization with
locations. Tokens include the build-specific numeric ID, symbolic name and
half-open byte span. Compare names across builds. Parse and scan diagnostics
carry SQLSTATE, message and PostgreSQL's character-based cursor position.
The probes run parsing and scanning independently, so a parse error can
still have a complete token stream.

The sorted inventory in `fixtures/cases.txt` lists each input name followed
by the optional probe arguments for that case.
Each `fixtures/*.sql` has a checked-in `.stock.json` and `.patched.json`.
The checker passes the file contents, including the final newline, to each
probe. When upgrading, change the release and digest, rebase the overlay,
and review changes in node fields, locations, errors and tokens before
replacing snapshots. Regenerate parser tables and review Rust reductions
when a production or its upstream C action changes.

`make -j4 test` exercises the existing C library. `cargo test --workspace`
runs the Rust tests. `python3 tools/pg19/tests/check-fixtures.py` checks
stored outcomes and token boundaries without building the probes. CI also
rebuilds both references when their inputs change. To run the full build through Cargo's test
runner, use `cargo test -p pg19-reference --test reference -- --ignored`. Set `PG19_ARCHIVE` to an
already downloaded archive for an offline run. CI passes the release
`pg19-native` executable as `PG19_NATIVE_PROBE` to this test. The test then
captures both references across the full regression corpus, verifies the
pinned digests and compares every patched outcome with Rust. CI also passes
the path to the release `deparse-corpus` example as `PG19_DEPARSE_PROBE`.
The PG19 JSON writer checks that every accepted SQL statement can be rendered
and reparsed with the same structural tree.

The native Rust scanner uses the Flex transitions from the patched build and
the release token and keyword catalogs. To verify the checked-in tables and
compare token IDs, byte spans and diagnostics across UTF-8 regression SQL
files, reuse a completed import.

```sh
python3 tools/pg19/import-lexer.py --check "$run"
PG19_REFERENCE_DIR="$run" cargo test -p pg19-lexer --test scan -- --ignored
```

Omit `--check` to regenerate the scanner data. The importer pins the hashes of
`scan.l` and `parser.c` so a change to lexical actions or grammar lookahead
requires reviewing their Rust implementations alongside the generated data.

The full reference test also regenerates the PG19 protobuf descriptor and
checks all release pins. It requires Ruby with Bundler and JSON, protoc,
and Python 3 with google.protobuf. To regenerate or verify the descriptor
from an existing import, see the [PG19 schema and wire compatibility review](../../protobuf/pg19/README.md).

The same build emits the C field writers used by the JSON serializer.
`check-schema` checks the Rust field mapping against those writers. To update
the mapping after reviewing an upstream change, run
`python3 crates/pg19-codec/generate-json.py "$schema_dir/.."`, where
`schema_dir` is the protobuf output directory printed by the schema command.
The checker links the C serializer to the patched PG19 backend and compares
its output byte for byte with the native codec fixtures in
`crates/pg19-codec/tests`. To inspect a difference, run the built
`"$schema_dir/../json-probe" 'SELECT 0'` with the SQL under review.

The fingerprint generator reads the same PG19 headers and applies the
repository's version 3 field-selection rules. The Rust visitor and the C
walker share the generator's field order and overrides. The checker rebuilds
the C walker and compares its seeded XXH3-64 digests with the versioned
fixtures in `crates/pg19-operations/tests/fingerprint-v3.jsonl`. To inspect a
digest, pipe SQL into `"$schema_dir/../fingerprint-probe" 0 0`.
The arguments are the fingerprint option bits and PostgreSQL raw parse mode.

Normalization uses the patched raw expression walker's field order. After an
upstream update, run `python3 tools/pg19/generate-raw-walk.py "$run"` and
review the ordered child fields alongside `src/pg_query_normalize.c`.
The clean reference test runs the full and utility normalization fixtures
against the C operation linked to PG19. The Rust tests exercise the same
inputs through the native SQL parser.

## Comparing native Rust trees

The reference tool's Rust library exposes `pg19_reference::node_text(&Node)`,
`nodes_text(&[Node])`, and `statements_text(&[RawStmt])`. Each returns
`Result<String, TreeError>`. Use `nodes_text` for a raw parse result in any
mode, including a list of type names. Compare its UTF-8 bytes directly with
the probe's decoded JSON `tree` string. Empty parse results print as `<>`.
The writer keeps pending fields on a heap stack so nested expressions can be
compared on the calling thread.

After importing PostgreSQL, verify the field writers and live snapshots with

```sh
python3 tools/pg19/generate-tree-output.py "$run" --check
PG19_REFERENCE_DIR="$run" cargo test -p pg19-reference --test native_tree -- --include-ignored
```

Omit `--check` to regenerate. The generator reads the patched backend's
`outfuncs.funcs.c`, `outfuncs.switch.c` and `outfuncs.c`. When the recorded
`outfuncs.c` digest changes, review its token, scalar and custom node writers
against the Rust code in `tools/pg19/src/tree`.

For full upstream SQL statement coverage, the [PG19 regression corpus](regress/README.md)
extracts source spans with the pinned psql lexer, captures trees and errors
from both references, and compares a candidate parser against either build.

The first-party SQL action generator reads Bison's expansion of the patched
`gram.y`. It recognizes complete single assignments such as copying a
semantic value, creating a scalar node or appending to an ordered list.
Each generated mapping carries the production and upstream action digest.
Reviewed actions in `crates/pg19-parser/actions/` take precedence. The
native probe reports the first missing production reached by each case.

```sh
python3 tools/pg19/generate-simple-actions.py "$run/patched-source" --write
cargo test --locked -p pg19-parser
```

Omit `--write` to check the generated file. On a version update, review
the table inventory and action-manifest differences together, then rerun
the corpus comparison against the freshly built patched reference.

The [PL/pgSQL reference](plpgsql/README.md) compiles function, procedure
and DO bodies with the patched PG19 compiler and the library's offline
catalog adapters. It emits routine JSON and SQL or body diagnostics.

The summary oracle links the library’s summary walkers, truncation algorithm, compact deparser and protobuf encoder with the patched PG19 backend. It reads one SQL input on standard input. The first argument selects the upstream raw parse mode (0 through 5). The optional second argument gives the truncation limit, with -1 as the default. A third argument of 16 disables backslash quoting. The optional final argument `text` prints the truncated SQL instead of the SummaryResult wire message.

```sh
python3 tools/pg19/tests/build-summary-probe.py "$run" "$run/summary"
python3 tools/pg19/tests/check-summary.py "$run/summary/summary-probe"
printf '%s' 'SELECT a, b, c, d, e, f FROM xyz WHERE a = b' | "$run/summary/summary-probe" 0 40 0 text
cargo test --locked -p pg19-operations --test summary
```

The build adapts the reference in the import directory for PG19’s REPACK and WAIT statements, publication targets and subscription commands. It also preserves quoted utility identifiers, numeric database options and database collation refreshes. The fixture check reads the original C summary and truncation tests, verifies their expected truncated SQL, and compares the resulting wire messages and diagnostics with the checked-in snapshots. Use `check-summary.py --write` with the rebuilt probe when reviewing a version change.

To repeat the full entity and diagnostic comparison, point the ignored integration test at the extracted regression corpus and the probe.

```sh
PG19_CORPUS="$corpus/cases.jsonl" PG19_SUMMARY_PROBE="$run/summary/summary-probe" \
  cargo test --locked -p pg19-operations --test summary compare_extracted_corpus -- --ignored --nocapture
```

## Compare native parser performance with C

After a PG19 reference import, schema generation, and regression capture, run

```sh
sh tools/pg19/bench/check.sh "$run" "$corpus" /tmp/pg19-benchmark
```

The command builds the public Rust parser with ordinary `cargo build --release`
settings and the patched PostgreSQL backend with `CFLAGS=-O3`. Both use the
system allocator. The script reuses an optimized
import or builds one in the output directory. Both programs parse in process,
destroy each result before the next iteration, and exclude process startup,
input loading and warmup from their timers.

Pass an existing benchmark executable as the fourth argument to compare a
separate build on the same inputs.

The benchmark checks byte-identical JSON for every input, then runs seven
paired rounds pinned to one allowed logical CPU. It alternates which
implementation runs first. Each mode measures `SELECT 1`, a parameterized
aggregate join, and every successful default-mode regression input. The
command exits unsuccessfully if the Rust median exceeds the C median on any
workload for either parse or parse plus JSON. The output directory holds the
per-round measurements in `samples.csv`, the six ratios in `results.json`,
and the input, binary and output digests in `machine-and-verification.json`.
`toolchain.txt` records the Rust and C compiler versions, PostgreSQL build
flags and CPU model. For measured release times and where both parsers spend
their cycles, see the [raw parser profile](bench/raw-profile.md).
