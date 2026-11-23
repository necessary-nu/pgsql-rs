# pgsql-syntax for PostgreSQL 19

This crate ports the pg_query.rs 18.1.0 API to the native Rust `pgsql-parser` backend. Add it as a path dependency from this checkout. Cargo generates the public prost types from the pinned PostgreSQL 19 schema. [Upstream revision](https://github.com/pganalyze/pg_query.rs/tree/c1f97cdf591ecdf2c57548147fa2dd0c35fbf795)

```toml
[dependencies]
pgsql-syntax = { path = "path/to/libpg_query/crates/pgsql-syntax" }
```

```rust
let result = pgsql_syntax::parse("SELECT * FROM contacts", 0)?;
assert_eq!(result.tables(), ["contacts"]);
assert_eq!(result.deparse(Default::default())?, "SELECT * FROM contacts");

let wait = pgsql_syntax::parse("WAIT FOR LSN '0/1'", 0)?;
assert_eq!(wait.statement_types(), ["WaitStmt"]);
# Ok::<(), pgsql_syntax::Error>(())
```

`parse` returns an owned tree and a metadata snapshot. Use `summary(sql, options, -1)` for metadata alone or `analyze(&tree)` for fresh metadata after an edit. All three resolve CTE references within their SQL query block. They collect syntactic references in expressions, including subqueries in `LIMIT` and `RETURNING`. A summary’s statement types include nested statements. The parse result’s `statement_types()` lists top-level statements in input order.

```rust
use pgsql_syntax::{analyze, parse, visit_mut, NodeRefMut, ParseMode};

let mut parsed = parse("SELECT * FROM contacts", ParseMode::Sql)?;
visit_mut(&mut parsed.protobuf, |record| {
    if let NodeRefMut::RangeVar(table) = record {
        table.relname = "customers".into();
    }
});
assert_eq!(parsed.tables(), ["contacts"]);
assert_eq!(analyze(&parsed.protobuf)?.tables(), ["customers"]);
assert_eq!(parsed.deparse(Default::default())?, "SELECT * FROM customers");
# Ok::<(), pgsql_syntax::Error>(())
```

`visit` and `visit_mut` walk every PG19 protobuf record, including literals and directly embedded records. A mutable callback can replace child fields and resize lists, and traversal continues with the edited children. The upstream `nodes()` and unsafe `nodes_mut()` helpers retain their original traversal.

Use `decode_parse_result(bytes)` for external protobuf trees. It requires schema version `190000`. For locally built prost trees, `analyze`, `deparse`, and `truncate` also accept the default version of zero. `ParseResult::try_new` returns errors when constructing a metadata snapshot from an incompatible tree.

`ParseMode` selects one grammar. `ParseConfig` combines a mode with the independent `disable_backslash_quote` setting. The upstream integer options still work. The `detailed` module exposes SQLSTATE, a one-based character cursor when PostgreSQL supplies one, and PL/pgSQL body context. Scanner token offsets are byte offsets.

SQL syntax, generated node fields and fingerprints follow PostgreSQL 19. For example, `CLUSTER` and `REPACK` both produce a `RepackStmt`.

Run `cargo test --locked -p pgsql-syntax` from the workspace root to exercise the upstream tests and the PG19 compatibility cases. To compare the generated types with the native codec across the extracted regression corpus, run `PG19_CORPUS=/path/to/cases.jsonl cargo test --release --locked -p pgsql-syntax --test corpus -- --ignored --nocapture`.
