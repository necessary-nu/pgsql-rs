use pg19_engine::ParseMode;
use pg19_lexer::Options;
use pg19_parser::{parse, Error};
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
                let tree = pg19_reference::nodes_text(&nodes).unwrap();
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
fn unsupported_actions_report_the_upstream_rule() {
    let error = parse(
        "SELECT CASE WHEN x THEN y END",
        ParseMode::Sql,
        Options::default(),
    )
    .unwrap_err();
    let Error::Unmapped(rule) = error else {
        panic!("{error}")
    };
    assert!(rule.action.is_some());
    assert!(rule.line > 0);
    assert!(rule.production.contains(':'));
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
