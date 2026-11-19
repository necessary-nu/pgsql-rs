use pg19_query::codec::protobuf;
use pg19_query::fingerprint::{fingerprint_sql, FingerprintOptions};
use pg19_query::{codec, deparse, lexer, nodes, Options, ParseMode};
use std::error::Error;

#[test]
fn public_query_operations() -> Result<(), Box<dyn Error>> {
    let sql = "SELECT 1; WAIT FOR LSN '0/1'";
    let opts = Options::default();
    let statements = pg19_query::parse_statements(sql, opts)?;
    assert!(matches!(
        statements[1].stmt.as_deref(),
        Some(nodes::Node::WaitStmt(_))
    ));
    assert!(codec::statements_json(&statements)?.contains("WaitStmt"));
    let bytes = protobuf::statements_protobuf(&statements)?;
    assert_eq!(protobuf::statements_from_protobuf(&bytes)?, statements);
    assert_eq!(deparse::protobuf(&bytes)?, sql);
    let tokens = lexer::Scanner::new(sql).collect::<Result<Vec<_>, _>>()?;
    let encoded = protobuf::scan_protobuf(&tokens)?;
    assert_eq!(protobuf::scan_from_protobuf(&encoded)?.len(), tokens.len());
    assert_eq!(pg19_query::split_with_parser(sql)?, [0..8, 10..sql.len()]);
    assert_eq!(pg19_query::split_with_scanner(sql)?, [0..8, 9..sql.len()]);
    assert_eq!(pg19_query::utility_statements(sql)?, [false, true]);
    assert_eq!(pg19_query::normalize(sql)?, "SELECT $1; WAIT FOR LSN '0/1'");
    assert_eq!(
        pg19_query::normalize_utility("CREATE ROLE r PASSWORD 'key'")?,
        "CREATE ROLE r PASSWORD $1"
    );
    let fp = fingerprint_sql(
        "SELECT 1",
        ParseMode::Sql,
        opts,
        FingerprintOptions::default(),
    )?;
    assert_eq!(
        fp,
        fingerprint_sql("SELECT $1", ParseMode::Sql, opts, Default::default())?
    );
    let routines = pg19_query::parse_routines("DO $$ BEGIN PERFORM 1; END $$;", opts)?;
    assert_eq!(routines.len(), 1);
    assert!(routines[0].action.is_some());
    assert!(pg19_query::parse_statements("SELECT )", opts).is_err());
    Ok(())
}
