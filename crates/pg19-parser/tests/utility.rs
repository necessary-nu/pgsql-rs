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

#[test]
fn deeply_nested_argument_types_are_copied_independently() {
    use pg19_nodes::Node;
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let sql = format!("DROP FUNCTION f(custom_type(1{}))", "+1".repeat(20_000));
            let parsed = parse(&sql, ParseMode::Sql, Options::default()).unwrap();
            let [Node::RawStmt(raw)] = parsed.as_slice() else {
                panic!()
            };
            let Some(Node::DropStmt(drop)) = raw.stmt.as_deref() else {
                panic!()
            };
            let [Node::ObjectWithArgs(signature)] = drop.objects.as_slice() else {
                panic!()
            };
            let [Node::TypeName(ty)] = signature.objargs.as_slice() else {
                panic!()
            };
            let [Node::FunctionParameter(param)] = signature.objfuncargs.as_slice() else {
                panic!()
            };
            let original = param.arg_type.as_deref().unwrap();
            assert!(!std::ptr::eq(ty.as_ref(), original));
            for ty in [ty.as_ref(), original] {
                let mut expr = &ty.typmods[0];
                let mut depth = 0;
                while let Node::AExpr(operation) = expr {
                    depth += 1;
                    expr = operation.lexpr.as_deref().unwrap();
                }
                assert_eq!(depth, 20_000);
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn ordered_variadic_arguments_compare_deep_type_modifiers() {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let modifier = format!("1{}", "+1".repeat(20_000));
            let arg = format!("VARIADIC custom_type({modifier})");
            let sql = format!("DROP AGGREGATE test({arg} ORDER BY {arg})");
            let parsed = parse(&sql, ParseMode::Sql, Options::default()).unwrap();
            let [pg19_nodes::Node::RawStmt(stmt)] = parsed.as_slice() else {
                panic!()
            };
            let Some(pg19_nodes::Node::DropStmt(drop)) = stmt.stmt.as_deref() else {
                panic!()
            };
            let [pg19_nodes::Node::ObjectWithArgs(sig)] = drop.objects.as_slice() else {
                panic!()
            };
            assert_eq!(sig.objargs.len(), 1);
            assert_eq!(sig.objfuncargs.len(), 1);
        })
        .unwrap()
        .join()
        .unwrap();
}
