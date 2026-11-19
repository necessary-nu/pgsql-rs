use pg19_nodes::*;
use pg19_operations::fingerprint::{fingerprint_sql, FingerprintOptions};
use pg19_operations::{
    is_utility_statement, split_with_parser, split_with_scanner, utility_statements,
};
use pg19_parser::{parse_statements, Error, Options, ParseMode};

// [spec:pgq:req:split.parser/test]
// [spec:pgq:req:split.boundaries/test]
#[test]
fn split_parsed_sql_using_raw_statement_boundaries() {
    assert_eq!(split_with_parser("").unwrap(), []);
    let sql = "SELECT 1; SELECT 2";
    assert_eq!(split_with_scanner(sql).unwrap(), [0..8, 9..18]);
    assert_eq!(split_with_parser(sql).unwrap(), [0..8, 10..18]);
    let sql = "select 'é;'; /* ; */ select 2   \0ignored";
    let ranges = split_with_parser(sql).unwrap();
    assert_eq!(
        ranges.iter().map(|r| &sql[r.clone()]).collect::<Vec<_>>(),
        ["select 'é;'", "select 2   "]
    );
    let invalid = "SELECT 1; SELECT a, )";
    let diagnostic = parse_statements(invalid, Options::default()).unwrap_err();
    assert!(matches!(diagnostic, Error::Diagnostic(_)));
    assert_eq!(split_with_parser(invalid).unwrap_err(), diagnostic);
    assert_eq!(utility_statements(invalid).unwrap_err(), diagnostic);
    let sql = "CREATE RULE x AS ON SELECT TO tbl DO (; SELECT 1; SELECT 2;); SELECT ';' /* ; */";
    let semi = sql.find(");").unwrap() + 1;
    assert_eq!(
        split_with_parser(sql).unwrap(),
        [0..semi, semi + 2..sql.len()]
    );
    assert_eq!(
        split_with_scanner(sql).unwrap(),
        [0..semi, semi + 1..sql.len()]
    );
    let stmts = parse_statements(sql, Options::default()).unwrap();
    let Some(Node::RuleStmt(rule)) = stmts[0].stmt.as_deref() else {
        panic!("missing RuleStmt")
    };
    assert_eq!(rule.actions.len(), 2);
    assert!(rule
        .actions
        .iter()
        .all(|n| matches!(n, Node::SelectStmt(_))));
}

// [spec:pgq:req:utility.classify/test]
// [spec:pgq:req:utility.sequence/test]
#[test]
fn classify_each_statement_by_its_outer_tag() {
    assert!(utility_statements("").unwrap().is_empty());
    assert_eq!(
        utility_statements(
            "WITH c AS (SELECT 1) SELECT * FROM c; WITH c AS (SELECT 1) UPDATE t SET x = 1; EXPLAIN SELECT 1; REPACK t"
        ).unwrap(),
        [false, false, true, true]
    );
    assert_eq!(
        utility_statements("SELECT 1; SHOW fsync").unwrap(),
        [false, true]
    );
    assert_eq!(
        utility_statements("; SELECT 1; ; SELECT 'SHOW';").unwrap(),
        [false, false]
    );
    assert!(utility_statements("/* SHOW */").unwrap().is_empty());
    for node in [
        SelectStmt::default().into(),
        UpdateStmt::default().into(),
        DeleteStmt::default().into(),
        InsertStmt::default().into(),
        MergeStmt::default().into(),
    ] {
        assert!(!is_utility_statement(&node));
    }
    assert!(is_utility_statement(
        &VariableShowStmt {
            name: Some("fsync".into())
        }
        .into()
    ));
    assert!(is_utility_statement(&RepackStmt::default().into()));
}

// [spec:pgq:req:fingerprint.constants/test]
#[test]
fn fingerprint_constants_and_parse_errors() {
    for sql in ["SELECT 1", "SELECT 2", "SELECT $1"] {
        assert_eq!(
            fingerprint_sql(
                sql,
                ParseMode::Sql,
                Options::default(),
                FingerprintOptions::default()
            )
            .unwrap()
            .to_string(),
            "50fde20626009aba"
        );
    }
    for (mode, expected) in [
        (ParseMode::TypeName, "57b8c5aa0601e31e"),
        (ParseMode::PlPgSqlExpr, "2ef767b7c9008694"),
    ] {
        let digest =
            fingerprint_sql("x", mode, Options::default(), FingerprintOptions::default()).unwrap();
        assert_eq!(digest.to_string(), expected);
    }
    let sql = "SELECT (";
    let err = parse_statements(sql, Options::default()).unwrap_err();
    assert_eq!(
        fingerprint_sql(
            sql,
            ParseMode::Sql,
            Options::default(),
            FingerprintOptions::default()
        )
        .unwrap_err(),
        err
    );
}

#[test]
fn shared_assignments_match_pg19_encodings_and_fingerprint() {
    let sql = "UPDATE t SET (a, b) = (SELECT r.x, r.y FROM r), c = 1";
    let statements = parse_statements(sql, Options::default()).unwrap();
    let Node::UpdateStmt(update) = statements[0].stmt.as_deref().unwrap() else {
        unreachable!()
    };
    let source = |column: usize| {
        let Node::ResTarget(target) = &update.target_list[column] else {
            unreachable!()
        };
        let Some(Node::MultiAssignRef(assignment)) = target.val.as_deref() else {
            unreachable!()
        };
        assignment.source.as_ref().unwrap()
    };
    assert!(std::sync::Arc::ptr_eq(source(0), source(1)));
    assert_eq!(
        pg19_codec::statements_json(&statements).unwrap(),
        include_str!("multi-assign.json").trim_end()
    );
    let binary = pg19_codec::protobuf::statements_protobuf(&statements).unwrap();
    assert_eq!(
        pg19_codec::protobuf::statements_from_protobuf(&binary).unwrap(),
        statements
    );
    assert_eq!(
        pg19_operations::fingerprint::fingerprint_statements(
            &statements,
            FingerprintOptions::default()
        )
        .to_string(),
        "ed222fbaa1c503de"
    );
}
