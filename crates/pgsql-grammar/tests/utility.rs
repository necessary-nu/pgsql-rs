use pgsql_engine::ParseMode;
use pgsql_grammar::{parse, Error};
use pgsql_lexer::Options;
use serde_json::{json, Value};

// [spec:pgq:sem:grammar.utility-actions/test]
#[test]
fn utility_trees_match_patched_postgres() {
    for line in include_str!("utility.jsonl").lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        let sql = case["sql"].as_str().unwrap();
        match parse(sql, ParseMode::Sql, Options::default()) {
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
    use pgsql_nodes::Node;
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
            let [pgsql_nodes::Node::RawStmt(stmt)] = parsed.as_slice() else {
                panic!()
            };
            let Some(pgsql_nodes::Node::DropStmt(drop)) = stmt.stmt.as_deref() else {
                panic!()
            };
            let [pgsql_nodes::Node::ObjectWithArgs(sig)] = drop.objects.as_slice() else {
                panic!()
            };
            assert_eq!(sig.objargs.len(), 1);
            assert_eq!(sig.objfuncargs.len(), 1);
        })
        .unwrap()
        .join()
        .unwrap();
}

// [spec:pgq:sem:grammar.utility-actions/test]
#[test]
fn utility_statements_preserve_options_and_byte_locations() {
    use pgsql_nodes::{Node, RepackCommand, TransactionStmtKind};
    let sql = "/* é */ WAIT FOR LSN '0/16B6C50'; REPACK t USING INDEX i; CREATE TABLE x (a int, b text); BEGIN ISOLATION LEVEL SERIALIZABLE, READ ONLY;";
    let statements = pgsql_grammar::parse_statements(sql, Options::default()).unwrap();
    let [wait, repack, create, begin] = statements.as_slice() else {
        panic!("expected four statements")
    };
    let Some(Node::WaitStmt(wait)) = wait.stmt.as_deref() else {
        panic!("expected WAIT")
    };
    assert_eq!(wait.lsn_literal.as_deref(), Some("0/16B6C50"));
    assert_eq!(wait.lsn_location.byte_offset(), sql.find("'0/16"));
    let Some(Node::RepackStmt(repack)) = repack.stmt.as_deref() else {
        panic!("expected REPACK")
    };
    assert_eq!(repack.command, RepackCommand::REPACK_COMMAND_REPACK);
    assert!(repack.usingindex);
    assert_eq!(repack.indexname.as_deref(), Some("i"));
    let rel = repack.relation.as_ref().unwrap().relation.as_ref().unwrap();
    assert_eq!(rel.relname.as_deref(), Some("t"));
    assert_eq!(rel.location.byte_offset(), sql.find("t USING"));
    let Some(Node::CreateStmt(create)) = create.stmt.as_deref() else {
        panic!("expected CREATE TABLE")
    };
    assert_eq!(
        create.relation.as_ref().unwrap().relname.as_deref(),
        Some("x")
    );
    let [Node::ColumnDef(a), Node::ColumnDef(b)] = create.table_elts.as_slice() else {
        panic!("expected two column definitions")
    };
    assert_eq!(
        (a.colname.as_deref(), b.colname.as_deref()),
        (Some("a"), Some("b"))
    );
    assert_eq!(a.location.byte_offset(), sql.find("a int"));
    assert_eq!(b.location.byte_offset(), sql.find("b text"));
    let Some(Node::TransactionStmt(begin)) = begin.stmt.as_deref() else {
        panic!("expected BEGIN")
    };
    assert_eq!(begin.kind, TransactionStmtKind::TRANS_STMT_BEGIN);
    let [Node::DefElem(isolation), Node::DefElem(readonly)] = begin.options.as_slice() else {
        panic!("expected two transaction options")
    };
    assert_eq!(isolation.defname.as_deref(), Some("transaction_isolation"));
    assert_eq!(isolation.location.byte_offset(), sql.find("ISOLATION"));
    assert_eq!(readonly.defname.as_deref(), Some("transaction_read_only"));
    assert_eq!(readonly.location.byte_offset(), sql.find("READ ONLY"));
}
