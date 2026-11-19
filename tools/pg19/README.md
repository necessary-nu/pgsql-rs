# PostgreSQL 19.0 reference parsers

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
already downloaded archive for an offline run.

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
The JSON fixtures in `crates/pg19-codec/tests` capture the C serializer linked
to the patched PG19 backend for `SELECT DISTINCT 0, NULL, false, '', '<é>',
B'10', 0.50, 7;` and `REPACK t USING INDEX i; WAIT FOR LSN '0/16B6C50'`.

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
