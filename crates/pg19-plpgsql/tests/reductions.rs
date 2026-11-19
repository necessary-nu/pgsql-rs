use pg19_plpgsql::{
    parse_body, token, Actions, Error, Input, Namespace, Options, ParseMode, Production, Reduction,
    Resolved, Token,
};

#[derive(Default)]
struct Collector {
    fragments: Vec<(String, ParseMode)>,
    seen_type: Option<String>,
}

impl Namespace for Collector {
    fn lookup(&mut self, names: &[String]) -> Option<Resolved> {
        (names == ["v"] && self.seen_type.is_some()).then_some(Resolved {
            datum: 1,
            names_used: 1,
        })
    }
}

impl Actions for Collector {
    type Value = Option<Token>;
    type Error = Error;

    fn shift(&mut self, token: Token) -> Self::Value {
        Some(token)
    }

    fn reduce(
        &mut self,
        rule: Production,
        rhs: &mut Reduction<Self::Value>,
        input: &mut Input,
    ) -> Result<Self::Value, Error> {
        let sig = rule.info().production;
        let s = &mut input.scanner;
        match sig {
            "decl_start: K_DECLARE" => s.lookup = pg19_plpgsql::Lookup::Declare,
            "decl_datatype: %empty" => {
                let t = input
                    .lookahead
                    .take()
                    .expect("type token was fetched to resolve decl_const");
                self.seen_type = Some(s.source()[t.span].to_owned());
            }
            "decl_sect: opt_block_label decl_start decl_stmts" => {
                s.lookup = pg19_plpgsql::Lookup::Normal
            }
            "stmt_assign: T_DATUM" => {
                s.push_back(rhs.take(1).unwrap());
                let f = s.read_sql(self, &[59], ";", ParseMode::PlPgSqlAssign1, false)?;
                self.fragments.push((f.query.to_owned(), f.mode));
            }
            "expr_until_then: %empty" => {
                let f = s.read_sql(self, &[token::K_THEN], "THEN", ParseMode::PlPgSqlExpr, true)?;
                self.fragments.push((f.query.to_owned(), f.mode));
            }
            _ => {}
        }
        Ok(rhs.default_value())
    }
}

#[test]
fn reductions_share_lexer_with_typenames_and_embedded_sql() {
    let src = "DECLARE v text; BEGIN IF v IS NOT NULL THEN v := (v || ';'); END IF; END;";
    let mut actions = Collector::default();
    parse_body(src, Options::default(), &mut actions).unwrap();
    assert_eq!(actions.seen_type.as_deref(), Some("text"));
    assert_eq!(
        actions.fragments,
        [
            ("v IS NOT NULL".to_owned(), ParseMode::PlPgSqlExpr),
            ("v := (v || ';')".to_owned(), ParseMode::PlPgSqlAssign1)
        ]
    );
}

#[test]
fn body_errors_report_utf8_character_positions() {
    let src = "BEGIN IF 'é' (= 2 THEN NULL; END IF; END";
    let err = parse_body(src, Options::default(), &mut Collector::default()).unwrap_err();
    assert_eq!(err.message, "mismatched parentheses at or near \";\"");
    assert_eq!(
        err.position,
        src[..src.find(';').unwrap()].chars().count() + 1
    );
}
