use pg19_nodes::*;
use pg19_operations::fingerprint::{fingerprint_sql, FingerprintOptions};
use pg19_operations::{
    is_utility_statement, split_with_parser, split_with_scanner, utility_statements,
};
use pg19_parser::{parse_statements, Error, Options, ParseMode};

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
}

#[test]
fn classify_each_statement_by_its_outer_tag() {
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
