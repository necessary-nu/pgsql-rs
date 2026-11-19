use pg19_nodes::{Node, NodeTag};
use pg19_parser::{parse, parse_statements, Error, Options, ParseMode};
use serde_json::Value;
use std::path::Path;

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
