use pgsql_engine::ParseMode;
use pgsql_grammar::{parse, Error};
use pgsql_lexer::Options;
use serde_json::{json, Value};

fn mode(case: &Value) -> (&str, ParseMode) {
    let name = case["mode"].as_str().unwrap_or("default");
    (
        name,
        match name {
            "default" => ParseMode::Sql,
            "type-name" => ParseMode::TypeName,
            "plpgsql-expr" => ParseMode::PlPgSqlExpr,
            "plpgsql-assign1" => ParseMode::PlPgSqlAssign1,
            "plpgsql-assign2" => ParseMode::PlPgSqlAssign2,
            "plpgsql-assign3" => ParseMode::PlPgSqlAssign3,
            _ => panic!("unknown PG19 mode {name}"),
        },
    )
}

#[test]
fn expression_trees_match_patched_postgres() {
    for line in include_str!("expressions.jsonl").lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        let sql = case["sql"].as_str().unwrap();
        match parse(sql, mode(&case).1, Options::default()) {
            Ok(nodes) => {
                let tree = pgsql_reference::nodes_text(&nodes).unwrap();
                assert_eq!(case["tree"], tree, "{sql}");
                assert!(case["error"].is_null());
            }
            Err(Error::Diagnostic(e)) => {
                assert_eq!(
                    case["error"],
                    json!({"sqlstate":e.sqlstate,"message":e.message,"position":e.position}),
                    "{sql}"
                );
                assert!(case["tree"].is_null());
            }
            Err(e) => panic!("{sql}: {e}"),
        }
    }
}

#[test]
#[ignore = "set PG19_REFERENCE_DIR to a built patched PostgreSQL oracle"]
fn verify_expression_snapshots() {
    let probe = std::path::PathBuf::from(std::env::var_os("PG19_REFERENCE_DIR").unwrap())
        .join("patched/probe");
    for line in include_str!("expressions.jsonl").lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        let sql = case["sql"].as_str().unwrap();
        let run = std::process::Command::new(&probe)
            .args([sql, "--mode", mode(&case).0])
            .output()
            .unwrap();
        assert!(run.status.success(), "{sql}");
        let result: Value = serde_json::from_slice(&run.stdout).unwrap();
        assert_eq!(result["tree"], case["tree"], "{sql}");
        assert_eq!(result["parse_error"], case["error"], "{sql}");
    }
}

// [spec:pgq:req:grammar.expression-binding/test]
#[test]
fn precedence_and_parentheses_build_different_operator_trees() {
    use pgsql_nodes::{Literal, Node};
    fn notation(node: &Node) -> String {
        match node {
            Node::AConst(n) => match &n.val {
                Literal::Integer(v) => v.to_string(),
                x => panic!("unexpected literal {x:?}"),
            },
            Node::AExpr(e) => {
                let [Node::String(op)] = e.name.as_slice() else {
                    panic!("expected operator")
                };
                format!(
                    "({} {op} {})",
                    notation(e.lexpr.as_deref().unwrap()),
                    notation(e.rexpr.as_deref().unwrap())
                )
            }
            _ => panic!("expected arithmetic expression"),
        }
    }
    for (sql, expected) in [
        ("SELECT 1 + 2 * 3", "(1 + (2 * 3))"),
        ("SELECT (1 + 2) * 3", "((1 + 2) * 3)"),
        ("SELECT 8 - 4 - 2", "((8 - 4) - 2)"),
    ] {
        let nodes = parse(sql, ParseMode::Sql, Options::default()).unwrap();
        let [Node::RawStmt(raw)] = nodes.as_slice() else {
            panic!("expected one statement")
        };
        let Some(Node::SelectStmt(select)) = raw.stmt.as_deref() else {
            panic!("expected SELECT")
        };
        let [Node::ResTarget(target)] = select.target_list.as_slice() else {
            panic!("expected one expression")
        };
        assert_eq!(notation(target.val.as_deref().unwrap()), expected, "{sql}");
    }
}
