# PostgreSQL 19.0 PL/pgSQL reference

Build this development executable from a completed [PG19 reference import](../README.md).
It accepts UTF-8 SQL containing routines and DO blocks and returns their
libpg_query JSON in source order. The build uses the import's compiler,
GNU make and linker, plus Python 3 and Ruby.

```sh
python3 tools/pg19/plpgsql/build.py "$run"
"$run/plpgsql-probe" 'DO $$ BEGIN NULL; END $$;'
python3 tools/pg19/plpgsql/check.py "$run"
```

Each argument is a separate call in the same process. The executable emits
one JSON object per line. Success has a `functions` array and a null
`error`. For an error, `functions` is null and `phase` identifies `sql`
(outer parsing) or `plpgsql` (routine construction or compilation).
`position` is the public API cursor. The reference also captures the
backend's SQLSTATE and `internal_position`; a body diagnostic can point
into the body while `position` is zero. The `context` field carries the
upstream compilation and line information when available.

The executable uses the actual patched PG19 procedural grammar, scanner
and compiler with the repository's `pg_query_parse_plpgsql` entry point
and `plpgsqlToJSON` writer. The builder invokes the PG19 PL/pgSQL makefile
to generate the procedural-language headers and objects. It compiles
temporary copies of the catalog-dependent functions with the existing
embedding adapters from [libpg_query mocks](../libpg_query/scripts/mocks) and the extracted inline mock
bodies. It exposes the compiler callback to the same synthetic `pg_proc`
tuple used by libpg_query. The built-in type table comes from the PG19
build's `postgres.bki`, type OID and collation headers. Compiled bodies are
inspected without executing SQL, starting a server or connecting to a
catalog. SQL-language functions and non-PL/pgSQL DO bodies have the
empty-datum function placeholder produced by the libpg_query entry point.

The checker compares all 30 routines in [the sample SQL](../libpg_query/test/plpgsql_samples.sql) with
the original expected JSON. The 12 additional cases cover procedures, DO,
SQL-only input, declaration order, trigger NEW/OLD record fields, error
context and cursors, and PG19 `WAIT` and `REPACK` inside a body. Four cases
are byte slices of the pinned upstream PL/pgSQL regression files.
The checker runs the successful sample after the error cases in the same
process. To compare a native executable implementing this interface, pass
`--candidate /path/to/executable` to `check.py`.

The offline catalog has the same limitations as the library. For example,
a function signature whose return type depends on a table column's
`%TYPE` can produce SQLSTATE `XX000` and message `Not implemented`.
Local variable `%TYPE` and `%ROWTYPE` declarations use the library's
offline datatypes. [Fixtures and upstream source spans](fixtures/cases.json)

The native body scanner and LALR driver are in `crates/pg19-plpgsql`.
Regenerate or verify their 336 states, 255 production signatures/action
digests and keyword catalogs with Bison 3.8.2. Cargo builds read the
checked-in artifacts. The scanner test compares external token IDs and
byte spans with the PL/pgSQL scanner in the reference build.

```sh
python3 tools/pg19/generate-tables.py "$run/patched-source" --plpgsql
"$run/plpgsql-probe" --scan 'BEGIN PERFORM $1; END;'
PG19_REFERENCE_DIR="$run" cargo test --locked -p pg19-plpgsql -- --include-ignored
```

A compiler implements `Actions` and `Namespace` and calls `parse_body`
on the original routine body. Each reduction sees the pending Bison
lookahead and the live scanner. Datatype actions can take the pending
token. Expression and assignment actions consume their own terminator
through `read_sql`, which retains the original byte range and raw parse
mode. Record-field lookup can create datums while the scanner reads
embedded SQL. The compiler can pass each fragment to the SQL parser
for validation with that mode.

`compile_block` accepts a routine's initial variables in datum order
and returns a native block and ordered datum list. The caller can
supply type resolution and embedded SQL validation through `EmbeddedSql`.
The routine compiler handles extraction, parameter and trigger setup,
implicit returns and public JSON. Errors retain context, line numbers
and character positions supplied by the reference.

The standalone type catalog contains the 197 rows in PG19.0
`postgres.bki`, including the `oid8` and `_oid8` OIDs. Run
`ruby tools/pg19/plpgsql/import-types.rb "$run"` to verify it. The
generator reuses the library's `extract_pg_types` reader and PG19's
`generate-plerrcodes.pl` script to derive exception condition names,
including duplicate labels for different SQLSTATEs. `NativeSql`
uses the PG19 Rust parser for type-name and embedded SQL start modes
and propagates the identity of an unimplemented SQL action.

`parse_routines(source, options)` parses the outer SQL with
`pg19-parser`. `compile_routines(statements, options, sql)` also
accepts an existing list of native `RawStmt` nodes. Both select routine
and DO definitions in source order. Routine setup builds positional
and named parameter bindings, OUT rows, PL/pgSQL trigger promises and
the implicit FOUND variable. Function bodies retain scalar/record
returns and the routine compiler's implicit returns. The live fixture
check in `tests/routines.rs` compares the PG19 oracle to checked typed
inputs while upstream SQL action coverage advances.

```sh
cargo build --locked -p pg19-plpgsql --example routines
target/debug/examples/routines 'SELECT 1' 'DO $$ BEGIN NULL; END $$'
python3 tools/pg19/plpgsql/check.py "$run" --candidate "$PWD/target/debug/examples/routines"
```

The example emits the oracle's result and diagnostic fields for
supported inputs. It reports an `unsupported` record for unimplemented
SQL or PL/pgSQL actions, with the production identity when available.

The body compiler handles declarations, exception handlers, diagnostics,
conditionals, loops, cursors, returns, static and dynamic SQL, and
transaction control. The SQL statement reader preserves comments before a
terminating semicolon and tracks parentheses and nested SQL routine
definitions with BEGIN ATOMIC and CASE expressions.

The standalone Rust test `native_entrypoint_matches_the_complete_project_sample`
checks all 30 routines in one SQL call. The live `compare_project_sample_routines`
test also exercises the same native PL compiler with external SQL inputs. The PG19 probes supply
the outer routine inputs, parsed type names and validation of embedded
SQL. Rust builds each routine's datums, statements, expressions and
JSON. The test checks all 30 resulting trees against the corresponding
elements of [the expected sample tree](../libpg_query/test/plpgsql_samples.expected.json).

```sh
PG19_REFERENCE_DIR="$run" cargo test -p pg19-plpgsql --test routines compare_project_sample_routines -- --ignored --nocapture
```

The upstream PL regression comparison runs each SQL region from all 13
files in PG19.0's `src/pl/plpgsql/src/sql` through the Rust and C public
entry points. It compares the entire JSON result, including diagnostics
and character positions. It records psql input regions separately.
The output directory contains classified counts, a digest of the C results,
and source byte ranges with both outputs for every difference.

```sh
cargo build --locked -p pg19-plpgsql --example routines
python3 -B tools/pg19/plpgsql/regress.py "$run" target/debug/examples/routines /tmp/pg19-pl-regress
```

Pass `--require-parity` to return a failure status for any mismatch,
unsupported production, or process failure. The `--boundaries` option
reuses the psql boundary helper from the core regression corpus.

Pass `--core-corpus "$corpus"` with the outputs from `regress/extract.py`
and `regress/probes.py` to include PG19 core regression inputs whose
patched C raw tree contains a function, procedure or DO statement.
