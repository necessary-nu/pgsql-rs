use pgsql_engine::{parse, Actions, ParseError, ParseMode, Reduction};
use pgsql_lexer::{Kind, Options, Token};
use pgsql_tables::Production;
use std::convert::Infallible;

#[derive(Default)]
struct Trace {
    prefix: Option<Kind>,
    root: Option<&'static str>,
}

impl Actions for Trace {
    type Value = ();
    type Error = Infallible;
    fn shift(&mut self, token: Token) {
        if self.prefix.is_none() {
            self.prefix = Some(token.kind);
        }
    }

    fn reduce(&mut self, rule: Production, rhs: &mut Reduction<'_, ()>) -> Result<(), Infallible> {
        let production = rule.info().production;
        if let Some(tail) = production.strip_prefix("parse_toplevel: ") {
            self.root = Some(tail);
            if tail.starts_with("MODE_") {
                assert_eq!(rhs.location(1).as_pg(), 0);
                assert!(rhs.location(2).as_pg() > 0);
            }
        }
        Ok(())
    }
}

#[test]
fn each_mode_uses_its_grammar_start() {
    for (mode, sql, prefix, root) in [
        (
            ParseMode::Sql,
            "  SELECT unknown_column",
            Kind::SELECT,
            "stmtmulti",
        ),
        (
            ParseMode::TypeName,
            "  character varying(32)",
            Kind::MODE_TYPE_NAME,
            "MODE_TYPE_NAME Typename",
        ),
        (
            ParseMode::PlPgSqlExpr,
            "  v IS NULL",
            Kind::MODE_PLPGSQL_EXPR,
            "MODE_PLPGSQL_EXPR PLpgSQL_Expr",
        ),
        (
            ParseMode::PlPgSqlAssign1,
            "  v := 1",
            Kind::MODE_PLPGSQL_ASSIGN1,
            "MODE_PLPGSQL_ASSIGN1 PLAssignStmt",
        ),
        (
            ParseMode::PlPgSqlAssign2,
            "  v.x := 1",
            Kind::MODE_PLPGSQL_ASSIGN2,
            "MODE_PLPGSQL_ASSIGN2 PLAssignStmt",
        ),
        (
            ParseMode::PlPgSqlAssign3,
            "  v.x.y := 1",
            Kind::MODE_PLPGSQL_ASSIGN3,
            "MODE_PLPGSQL_ASSIGN3 PLAssignStmt",
        ),
    ] {
        let mut trace = Trace::default();
        parse(sql, mode, Options::default(), &mut trace).unwrap();
        assert_eq!((trace.prefix, trace.root), (Some(prefix), Some(root)));
    }
}

fn error(sql: &str) -> pgsql_lexer::Error {
    match parse(
        sql,
        ParseMode::Sql,
        Options::default(),
        &mut Trace::default(),
    ) {
        Err(ParseError::Diagnostic(e)) => e,
        other => panic!("expected a diagnostic, got {other:?}"),
    }
}

#[test]
fn rejects_at_token_or_end_of_input() {
    let sql = include_str!("../../../tools/pg19/fixtures/utf8-error.sql");
    let snapshot: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tools/pg19/fixtures/utf8-error.patched.json"
    ))
    .unwrap();
    let e = error(sql);
    assert_eq!(e.message, snapshot["parse_error"]["message"]);
    assert_eq!(
        e.position,
        snapshot["parse_error"]["position"].as_u64().unwrap() as usize
    );
    let e = error("SELECT (\0ignored");
    assert_eq!(e.message, "syntax error at end of input");
    assert_eq!(e.position, 9);
    let e = error("SELECT 1; SELECT (");
    assert_eq!(e.position, 19);
}

#[test]
fn stack_exhaustion_matches_reference() {
    let e = error(&format!("SELECT{}", "(".repeat(10020)));
    assert_eq!(e.message, "memory exhausted at or near \"(\"");
    assert_eq!(e.position, 10003);
}
