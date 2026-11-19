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
embedding adapters from `scripts/mocks` and the extracted inline mock
bodies. It exposes the compiler callback to the same synthetic `pg_proc`
tuple used by libpg_query. The built-in type table comes from the PG19
build's `postgres.bki`, type OID and collation headers. Compiled bodies are
inspected without executing SQL, starting a server or connecting to a
catalog. SQL-language functions and non-PL/pgSQL DO bodies have the
empty-datum function placeholder produced by the libpg_query entry point.

The checker compares all 30 routines in `test/plpgsql_samples.sql` with
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
