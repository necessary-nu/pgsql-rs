# PostgreSQL 19 parser for Rust

This Cargo workspace parses PostgreSQL 19 SQL in process. Add either public crate as a path dependency from this checkout.

| Crate | Use it for |
| --- | --- |
| [pgsql-parser](crates/pgsql-parser) | Parse SQL into an arena-backed raw tree, inspect statement spans, or serialize directly to JSON. |
| [pgsql-syntax](crates/pgsql-syntax/README.md) | Inspect and edit an owned syntax tree, analyze table references, deparse SQL, or compute a fingerprint. |

`parse_raw` returns a `RawTree` that owns its storage and can outlive the SQL string. Use `statements()` to borrow a statement, `json()` to serialize the tree directly, or `into_owned()` to create independently mutable nodes.

```rust
use pgsql_parser::{nodes::Node, parse_raw, Options, ParseError};

fn main() -> Result<(), ParseError> {
    let tree = parse_raw("REPACK accounts USING INDEX accounts_pkey", Options::default())?;
    let statements = tree.into_owned();
    if let Some(Node::RepackStmt(repack)) = statements[0].stmt.as_deref() {
        assert_eq!(repack.indexname.as_deref(), Some("accounts_pkey"));
    }
    Ok(())
}
```

## Development

All workspace packages live in `crates/`. The internal crates share the `pgsql-` prefix. The `pgsql-grammar` crate implements the SQL grammar used by `pgsql-parser`. The `pgsql-reference` development tool builds the stock and patched PostgreSQL references.

```sh
cargo build --locked
cargo test --locked --workspace
```

The workspace tests use `protoc` for wire-format checks. The public parser and syntax crates build from the checked-in grammar and schema.

The release importer, comparison probes and benchmarks live in `tools/pg19/`. Running the optional reference import checks out pinned libpg_query sources and builds PostgreSQL in the supplied work directory. The PG19 protobuf schema and its wire compatibility report live in `protobuf/pg19/`. [Grammar maintenance](tools/pg19/README.md) · [Schema history](protobuf/pg19/README.md) · [Rust and C benchmarks](tools/pg19/README.md#compare-native-parser-performance-with-c)

The workspace includes code from PostgreSQL, libpg_query and pg_query.rs. [PostgreSQL license](LICENSE-PostgreSQL) · [libpg_query license](LICENSE) · [pgsql-syntax license](crates/pgsql-syntax/LICENSE)
