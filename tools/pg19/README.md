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
cargo run -p pg19-reference -- compare "$run"
```

Each probe emits JSON. The tree is PostgreSQL's raw node serialization with
locations. Tokens include the build-specific numeric ID, symbolic name and
half-open byte span. Compare names across builds. Parse and scan diagnostics
carry SQLSTATE, message and PostgreSQL's character-based cursor position.
The probes run parsing and scanning independently, so a parse error can
still have a complete token stream.

The sorted inventory in `fixtures/cases.txt` names the required inputs.
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
runner, use `cargo test --workspace -- --ignored`. Set `PG19_ARCHIVE` to an
already downloaded archive for an offline run.

The full reference test also regenerates the PG19 protobuf descriptor and
checks all release pins. It requires Ruby with Bundler and JSON, protoc,
and Python 3 with google.protobuf. To regenerate or verify the descriptor
from an existing import, see the [PG19 schema and wire compatibility review](../../protobuf/pg19/README.md).
