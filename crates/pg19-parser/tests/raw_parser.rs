use pg19_nodes::{Node, NodeTag};
use pg19_parser::{parse, parse_statements, Error, Options, ParseMode};
use serde_json::Value;
use std::path::Path;

#[test]
fn final_semicolon_preserves_postgres_statement_spans() {
    for (sql, spans) in [
        (";;;", &[][..]),
        ("SELECT 1", &[(0, 0)][..]),
        ("SELECT é;", &[(0, 9)][..]),
        (" SELECT 1;;", &[(1, 8)][..]),
        (" SELECT 1; /* tail */", &[(1, 8)][..]),
        (" SELECT 1;\0ignored", &[(1, 8)][..]),
        (" SELECT 1; SELECT 2;", &[(1, 8), (11, 8)][..]),
    ] {
        let statements = parse_statements(sql, Options::default()).unwrap();
        assert_eq!(
            statements
                .iter()
                .map(|s| (s.stmt_location.as_pg(), s.stmt_len))
                .collect::<Vec<_>>(),
            spans,
            "{sql:?}"
        );
    }
    let Err(Error::Diagnostic(error)) = parse_statements("SELECT 1; /*", Options::default()) else {
        panic!("expected an unterminated comment");
    };
    assert_eq!(
        (error.message.as_str(), error.position),
        ("unterminated /* comment at or near \"/*\"", 11)
    );
}

#[test]
fn omitted_clauses_observe_bisons_stack_limit() {
    for (opens, message) in [
        (9987, "syntax error at end of input"),
        (9988, "memory exhausted at or near \")\""),
    ] {
        let sql = format!("SELECT {}SELECT 1)", "(".repeat(opens));
        let Err(Error::Diagnostic(error)) = parse_statements(&sql, Options::default()) else {
            panic!("expected a PG19 diagnostic at nesting depth {opens}");
        };
        assert_eq!((error.message.as_str(), error.position), (message, 10004));
    }
}

fn snapshot(name: &str, mode: ParseMode) -> Vec<Node> {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/pg19/fixtures");
    let sql = std::fs::read_to_string(fixtures.join(format!("{name}.sql"))).unwrap();
    let saved = std::fs::read(fixtures.join(format!("{name}.patched.json"))).unwrap();
    let saved: Value = serde_json::from_slice(&saved).unwrap();
    let nodes = parse(&sql, mode, Options::default()).unwrap();
    assert_eq!(
        pg19_reference::nodes_text(&nodes).unwrap(),
        saved["tree"],
        "{name}"
    );
    nodes
}

// [spec:pgq:req:grammar.raw-sql/test]
// [spec:pgq:req:grammar.runtime/test]
#[test]
fn independent_calls_keep_raw_results_after_later_errors() {
    let first = parse_statements("SELECT * FROM does_not_exist", Options::default()).unwrap();
    let Err(Error::Diagnostic(err)) = parse_statements("SELECT 1; SELECT )", Options::default())
    else {
        panic!("expected later statement to fail the parse")
    };
    assert_eq!((err.sqlstate, err.position), ("42601", 18));
    let second = parse_statements("BEGIN; COMMIT", Options::default()).unwrap();
    assert_eq!(second.len(), 2);
    let Some(Node::SelectStmt(select)) = first[0].stmt.as_deref() else {
        panic!("expected SELECT")
    };
    assert!(
        matches!(&select.from_clause[..], [Node::RangeVar(r)] if r.relname.as_deref() == Some("does_not_exist"))
    );
}

// [spec:pgq:req:grammar.parse-modes/test]
#[test]
fn raw_start_modes_match_reference_trees() {
    for (name, mode) in [
        ("repack", ParseMode::Sql),
        ("type-name", ParseMode::TypeName),
        ("expr", ParseMode::PlPgSqlExpr),
        ("assign1", ParseMode::PlPgSqlAssign1),
        ("assign2", ParseMode::PlPgSqlAssign2),
        ("assign3", ParseMode::PlPgSqlAssign3),
    ] {
        snapshot(name, mode);
    }
}

// [spec:pgq:req:grammar.overlay-differences/test]
#[test]
fn overlay_cases_match_stock_and_patched_references() {
    for (name, position) in [("param-junk", 8), ("set-role", 10)] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../tools/pg19/fixtures/{name}.stock.json"));
        let stock: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(stock["parse_error"]["sqlstate"], "42601");
        assert_eq!(stock["parse_error"]["position"], position);
        snapshot(name, ParseMode::Sql);
    }
}

// [spec:pgq:req:grammar.sql-families/test]
#[test]
fn sql_families_return_raw_statements_in_source_order() {
    let sql = "SELECT 1 UNION SELECT 2; MERGE INTO t USING u ON t.id = u.id WHEN MATCHED THEN DELETE; CREATE TABLE t(a int); BEGIN; REPACK t USING INDEX i; WAIT FOR LSN '0/1'";
    let stmts = parse_statements(sql, Default::default()).unwrap();
    let tags: Vec<_> = stmts
        .iter()
        .map(|s| s.stmt.as_deref().and_then(Node::tag).unwrap())
        .collect();
    assert_eq!(
        tags,
        [
            NodeTag::SelectStmt,
            NodeTag::MergeStmt,
            NodeTag::CreateStmt,
            NodeTag::TransactionStmt,
            NodeTag::RepackStmt,
            NodeTag::WaitStmt
        ]
    );
    let Some(Node::RepackStmt(repack)) = stmts[4].stmt.as_deref() else {
        unreachable!()
    };
    assert_eq!(repack.indexname.as_deref(), Some("i"));
    assert_eq!(
        repack
            .relation
            .as_ref()
            .unwrap()
            .relation
            .as_ref()
            .unwrap()
            .relname
            .as_deref(),
        Some("t")
    );
}

#[test]
fn live_trees_own_storage_across_threads() {
    use pg19_nodes::NodeTag;
    use pg19_parser::parse_raw;
    let first = {
        let sql = String::from("select 'first'; select 2");
        parse_raw(&sql, Default::default()).unwrap()
    };
    let second = parse_raw("delete from t", Default::default()).unwrap();
    assert!(parse_raw("select (", Default::default()).is_err());
    let first_stmt = first.statements().next().unwrap();
    assert_eq!(first_stmt.source_range(24), Some(0..14));
    std::thread::scope(|scope| {
        scope.spawn(|| assert!(first_stmt.json().unwrap().contains("first")));
        scope.spawn(move || {
            assert_eq!(
                second.statements().next().unwrap().tag(),
                Some(NodeTag::DeleteStmt)
            );
            drop(second);
        });
    });
    for _ in 0..32 {
        drop(parse_raw("select 123", Default::default()).unwrap());
    }
    let json = first.json().unwrap();
    let owned = first.into_owned();
    assert_eq!(owned.len(), 2);
    assert!(json.contains("first"));
    assert!(matches!(
        owned[0].stmt.as_deref(),
        Some(pg19_nodes::Node::SelectStmt(_))
    ));
}
