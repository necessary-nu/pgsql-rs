//! Differential check for the generated native/prost mapping.
use pgsql_parser::{codec::protobuf as wire, nodes, Options, ParseMode};
use serde::Deserialize;
use std::io::{BufRead, BufReader};

#[derive(Deserialize)]
struct Case {
    id: String,
    mode: String,
    backslash_quote: String,
    sql: String,
}

#[test]
#[ignore = "set PG19_CORPUS to the extracted PostgreSQL 19 cases.jsonl"]
fn typed_trees_match_the_native_codec() {
    let file = std::fs::File::open(std::env::var_os("PG19_CORPUS").unwrap()).unwrap();
    let (mut trees, mut diagnostics, mut deep) = (0, 0, 0);
    for line in BufReader::new(file).lines() {
        let case: Case = serde_json::from_str(&line.unwrap()).unwrap();
        let (mode, mut flags) = match case.mode.as_str() {
            "default" => (ParseMode::Sql, 0),
            "type-name" => (ParseMode::TypeName, 1),
            "plpgsql-expr" => (ParseMode::PlPgSqlExpr, 2),
            "plpgsql-assign1" => (ParseMode::PlPgSqlAssign1, 3),
            "plpgsql-assign2" => (ParseMode::PlPgSqlAssign2, 4),
            "plpgsql-assign3" => (ParseMode::PlPgSqlAssign3, 5),
            mode => panic!("unknown mode {mode}"),
        };
        let options = Options {
            disable_backslash_quote: case.backslash_quote == "off",
        };
        if options.disable_backslash_quote {
            flags |= 16;
        }
        let actual = pgsql_syntax::parse(&case.sql, flags);
        let nodes = match pgsql_parser::parse(&case.sql, mode, options) {
            Ok(v) => v,
            Err(e) => {
                assert_eq!(
                    actual.unwrap_err(),
                    pgsql_syntax::Error::Parse(e.to_string()),
                    "{}",
                    case.id
                );
                diagnostics += 1;
                continue;
            }
        };
        let stmts: Vec<_> = nodes
            .into_iter()
            .map(|n| match n {
                nodes::Node::RawStmt(s) => s.into_inner(),
                other => nodes::RawStmt::new(other, Default::default()),
            })
            .collect();
        let bytes = match wire::statements_protobuf(&stmts) {
            Ok(bytes) => bytes,
            Err(wire::ProtobufError::DepthLimit) => {
                // The general-purpose codec caps message nesting at 48. pgsql-syntax
                // supports prost's 100-level limit, so it can accept deeper trees.
                assert!(
                    actual.is_ok() || matches!(actual, Err(pgsql_syntax::Error::Decode(_))),
                    "{}",
                    case.id
                );
                deep += 1;
                continue;
            }
            Err(e) => panic!("{}: {e}", case.id),
        };
        let expected = pgsql_syntax::decode_parse_result(&bytes).unwrap();
        let typed = actual.unwrap_or_else(|e| panic!("{}: {e}", case.id));
        assert_eq!(typed.protobuf, expected, "{}", case.id);
        let sql_summary = pgsql_syntax::summary(&case.sql, flags, -1).unwrap();
        let from_tree = pgsql_syntax::analyze(&expected).unwrap();
        assert_eq!(sql_summary.protobuf, from_tree.protobuf, "{}", case.id);
        let native = pgsql_parser::deparse::protobuf(&bytes).map_err(|e| e.to_string());
        let rendered = typed.deparse(Default::default()).map_err(|e| match e {
            pgsql_syntax::Error::Parse(reason) => reason,
            other => other.to_string(),
        });
        assert_eq!(rendered, native, "{}", case.id);
        trees += 1;
    }
    assert!(trees > 0);
    eprintln!("{trees} typed trees, deparse results and analyses, {diagnostics} diagnostics, {deep} beyond the native codec depth limit");
}
