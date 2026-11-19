use pg19_engine::ParseMode;
use pg19_lexer::Options;
use pg19_parser::{parse, Error};
use serde_json::{json, Value};

#[test]
fn utility_trees_match_patched_postgres() {
    for line in include_str!("utility.jsonl").lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        let sql = case["sql"].as_str().unwrap();
        match parse(sql, ParseMode::Sql, Options::default()) {
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
#[ignore = "set PG19_REFERENCE_DIR to a built patched PostgreSQL oracle"]
fn verify_utility_snapshots() {
    let probe = std::path::PathBuf::from(std::env::var_os("PG19_REFERENCE_DIR").unwrap())
        .join("patched/probe");
    for line in include_str!("utility.jsonl").lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        let sql = case["sql"].as_str().unwrap();
        let run = std::process::Command::new(&probe)
            .arg(sql)
            .output()
            .unwrap();
        assert!(run.status.success(), "{sql}");
        let result: Value = serde_json::from_slice(&run.stdout).unwrap();
        assert_eq!(result["tree"], case["tree"], "{sql}");
        assert_eq!(result["parse_error"], case["error"], "{sql}");
    }
}
