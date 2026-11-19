use pg19_engine::{parse, Actions, ParseError, ParseMode, Reduction};
use pg19_lexer::{Kind, Options, Token, Value};
use pg19_nodes::{Location, Node, NotifyStmt, StatementList};
use pg19_tables::Production;

#[derive(Debug, Default)]
enum Semantic {
    #[default]
    Empty,
    Text(String),
    Token(Kind),
    Statement(Node),
    Statements(StatementList),
}

impl Semantic {
    fn text(self) -> String {
        let Self::Text(s) = self else {
            panic!("expected text")
        };
        s
    }

    fn statement(self) -> Option<Node> {
        match self {
            Self::Statement(node) => Some(node),
            Self::Empty => None,
            _ => panic!("expected statement or empty"),
        }
    }

    fn list(self) -> StatementList {
        let Self::Statements(s) = self else {
            panic!("expected statement list")
        };
        s
    }
}

struct Notify;

impl Actions for Notify {
    type Value = Semantic;
    type Error = &'static str;

    fn shift(&mut self, token: Token) -> Semantic {
        match token.value {
            Value::Text(s) => Semantic::Text(s),
            _ => Semantic::Token(token.kind),
        }
    }

    fn reduce(
        &mut self,
        rule: Production,
        rhs: &mut Reduction<'_, Semantic>,
    ) -> Result<Semantic, Self::Error> {
        let node = match rule.info().production {
            "NotifyStmt: NOTIFY ColId ',' Sconst" => NotifyStmt {
                conditionname: Some(rhs.take(2).text()),
                payload: Some(rhs.take(4).text()),
                payload_location: rhs.location(4),
            },
            "NotifyStmt: NOTIFY ColId" => NotifyStmt {
                conditionname: Some(rhs.take(2).text()),
                ..Default::default()
            },
            "stmt: %empty" => {
                assert_eq!(rhs.lhs_location, Location::UNKNOWN);
                if let Semantic::Token(previous) = rhs.value(0) {
                    assert_eq!(previous.name(), ";");
                }
                return Ok(Semantic::Empty);
            }
            "stmtmulti: toplevel_stmt" => {
                let mut list = StatementList::default();
                list.push(rhs.take(1).statement(), rhs.location(1));
                return Ok(Semantic::Statements(list));
            }
            "stmtmulti: stmtmulti ';' toplevel_stmt" => {
                let mut list = rhs.take(1).list();
                list.terminate_last_at(rhs.location(2)).unwrap();
                list.push(rhs.take(3).statement(), rhs.location(3));
                return Ok(Semantic::Statements(list));
            }
            "parse_toplevel: stmtmulti" | "ColId: IDENT" | "Sconst: SCONST" => {
                return Ok(rhs.default_value())
            }
            production => return Err(production),
        };
        Ok(Semantic::Statement(node.into()))
    }
}

fn tree(sql: &str) -> String {
    let mut rows = Vec::new();
    let parsed = parse(sql, ParseMode::Sql, Options::default(), &mut Notify).unwrap();
    for raw in parsed.list().into_statements() {
        let Node::NotifyStmt(n) = *raw.stmt.unwrap() else {
            panic!("expected NotifyStmt")
        };
        rows.push(format!(
            "{{RAWSTMT :stmt {{NOTIFYSTMT :conditionname {} :payload {} :payload_location {}}} :stmt_location {} :stmt_len {}}}",
            n.conditionname.as_deref().unwrap(), n.payload.as_deref().unwrap_or("<>"),
            n.payload_location.as_pg(), raw.stmt_location.as_pg(), raw.stmt_len,
        ));
    }
    if rows.is_empty() {
        "<>".into()
    } else {
        format!("({})", rows.join(" "))
    }
}

// [spec:pgq:req:grammar.reductions/test]
#[test]
fn notify_trees_match_patched_reference() {
    let sql = include_str!("../../../tools/pg19/fixtures/constant-location.sql");
    let snapshot: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tools/pg19/fixtures/constant-location.patched.json"
    ))
    .unwrap();
    assert_eq!(tree(sql), snapshot["tree"].as_str().unwrap());
    assert_eq!(tree(";;  NOTIFY \"é\", 'oui';; NOTIFY t"), concat!(
        "({RAWSTMT :stmt {NOTIFYSTMT :conditionname é :payload oui :payload_location 17} :stmt_location 4 :stmt_len 18} ",
        "{RAWSTMT :stmt {NOTIFYSTMT :conditionname t :payload <> :payload_location 0} :stmt_location 25 :stmt_len 0})"
    ));
    assert_eq!(tree("; /* trivia */ ;"), "<>");
}

#[test]
fn semantic_error_precedes_later_lexical_error() {
    let result = parse(
        "SELECT 1; /*",
        ParseMode::Sql,
        Options::default(),
        &mut Notify,
    );
    assert!(matches!(result, Err(ParseError::Action("Iconst: ICONST"))));
    assert_eq!(tree(""), "<>");
}

#[derive(Default)]
struct Locations {
    at_delimiter: Vec<(Location, Location)>,
    reductions: Vec<Location>,
}

impl Actions for Locations {
    type Value = Option<Kind>;
    type Error = std::convert::Infallible;

    fn shift(&mut self, token: Token) -> Option<Kind> {
        Some(token.kind)
    }

    fn reduce(
        &mut self,
        rule: Production,
        rhs: &mut Reduction<'_, Self::Value>,
    ) -> Result<Self::Value, Self::Error> {
        match rule.info().production {
            "stmt: %empty" if rhs.value(0).is_some() => {
                assert_eq!(rhs.value(0).unwrap().name(), ";");
                self.at_delimiter.push((rhs.location(0), rhs.location(-1)));
            }
            "stmtmulti: stmtmulti ';' toplevel_stmt" => {
                self.reductions.push(rhs.lhs_location);
                rhs.lhs_location = rhs.location(2);
            }
            "parse_toplevel: stmtmulti" => self.reductions.push(rhs.location(1)),
            _ => {}
        }
        Ok(rhs.default_value())
    }
}

#[test]
fn empty_symbols_keep_prefix_values_and_locations() {
    let mut actions = Locations::default();
    parse(" ;;", ParseMode::Sql, Options::default(), &mut actions).unwrap();
    let first = Location::from_pg(1).unwrap();
    let second = Location::from_pg(2).unwrap();
    assert_eq!(
        actions.at_delimiter,
        [(first, Location::UNKNOWN), (second, first)]
    );
    assert_eq!(actions.reductions, [first, first, second]);
}

struct InvalidColumnRef<'a>(&'a str);

impl Actions for InvalidColumnRef<'_> {
    type Value = ();
    type Error = pg19_lexer::Error;

    fn shift(&mut self, _: Token) {}

    fn reduce(&mut self, rule: Production, rhs: &mut Reduction<'_, ()>) -> Result<(), Self::Error> {
        if rule.info().production == "columnref: ColId indirection" {
            let (start, end) = rhs.scanner_span();
            return Err(pg19_lexer::Error::near(
                self.0,
                start,
                end,
                r#"improper use of "*""#,
            ));
        }
        Ok(())
    }
}

// [spec:pgq:req:grammar.reductions/test]
#[test]
fn action_diagnostics_follow_lookahead_and_eof() {
    // These diagnostics come from makeColumnRef in the patched 19.0 C reference.
    for (sql, message, position) in [
        (
            "SELECT é.*.b /* gap */ FROM t",
            r#"improper use of "*" at or near "FROM""#,
            24,
        ),
        (
            "SELECT a.*.b /* trailing */",
            r#"improper use of "*" at end of input"#,
            28,
        ),
    ] {
        let mut actions = InvalidColumnRef(sql);
        let Err(ParseError::Action(err)) =
            parse(sql, ParseMode::Sql, Options::default(), &mut actions)
        else {
            panic!("expected a column reference action error for {sql}");
        };
        assert_eq!(
            (err.sqlstate, err.message.as_str(), err.position),
            ("42601", message, position)
        );
    }
}
