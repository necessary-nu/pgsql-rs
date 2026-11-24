# PostgreSQL 19.0 protobuf schema

`pg_query.proto` defines the project’s libpg_query-compatible format for the
patched PostgreSQL 19.0 grammar. Both the parse and scan envelopes carry
version `190000`. The compiled `pg_query.pb` is a
`google.protobuf.FileDescriptorSet` containing `pg_query.proto` in package
`pg_query`. Its SHA-256 is
`55730f6fbe40a887a2ebbfa8314364c0749ae018063152287b4d2bb3e8956788`.

## Reproducing the descriptor

Run the reference importer, then pass the run directory it prints to the
schema checker. Schema generation additionally requires Ruby with Bundler
and JSON, protoc, and Python 3 with google.protobuf. The original capture
used Bison 3.8.2, Ruby 3.3.8, protoc 3.21.12 and Python protobuf 4.21.12.

```sh
cargo run -p pgsql-reference -- import /tmp/pg19-runs
cargo run -p pgsql-reference -- check-schema "$run"
```

The checker runs the libpg_query header extractor on the patched source
and its generated `nodetags.h` and `gram.h`. It passes the resulting node
layouts, typedefs and enum declarations to the repository’s protobuf
generator. That generator owns the envelopes, JSON names, field numbering,
node oneof, constant variants and enum mappings. Bison also emits four
internal symbols. The scan token enum includes the PostgreSQL token numbers
and single-byte tokens, with `NUL` at zero.

For an update, use `schema "$run"` in place of `check-schema "$run"` to
write candidates in a fresh directory. Compare the candidate schema and
descriptor to the preceding version. Review the machine-generated field
and enum differences in `changes.txt`, and record their effect on consumers
here. The four checked artifacts include the SHA-256 manifest of the
release archive, overlay patches, generators, extracted JSON, comparison
baseline and generated outputs. The fifth file is this review.

## Wire compatibility with PostgreSQL 18.6

Decode payloads with the descriptor for the producer’s version. Both majors
use the same package name and envelope, and reusing field numbers can silently
decode a valid payload as a different node. The PG18.6 schema is in
[the libpg_query reference inputs](https://github.com/pganalyze/libpg_query/blob/27112f8af5de70368319a12c346c2fb37cd74476/protobuf/pg_query.proto).

* The `Node` oneof grows from 271 to 273 arms. Field 235 changes from
  `ClusterStmt` to `VacuumStmt`. `RepackStmt` takes field 237,
  `PublicationAllObjSpec` takes 258 and `WaitStmt` takes 264. The
  `A_Const` arm moves from 271 to 273.
* `AlterDomainStmt.subtype` changes from a string to the `AlterDomainType`
  enum at field 1, changing its wire type. New fields also shift existing
  numbers in `FuncCall`, `OnConflictClause`, subscription statements and
  several other messages. For example, `FuncCall` field 6 now carries
  `ignore_nulls`, and `agg_within_group` moves to 7.
* Libpg_query maps C enum symbols to sequential wire values starting at 1
  and reserves zero for its undefined value. `ONCONFLICT_SELECT` is 4.
  New publication, subscription and JSON enum members shift existing values.
* The scan enum gains `IGNORE_P`, `LSN_P`, `REPACK`, `RESPECT_P` and
  `WAIT`. Existing token numbers shift in five ranges, including
  `WHEN` through `UMINUS`, which move up by five.

[Complete field, oneof and enum changes from 18.6](changes.txt)
