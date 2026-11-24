use pgsql_engine::{parse, Actions, ParseError, ParseMode, Reduction};
use pgsql_lexer::{Options, Token, Value};
use pgsql_nodes::{AExpr, Node};
use pgsql_tables::Production;
use std::convert::Infallible;

struct Additions(usize);

impl Actions for Additions {
    type Value = Option<Node>;
    type Error = Infallible;

    fn shift(&mut self, token: Token) -> Option<Node> {
        match token.value {
            Value::Integer(i) => Some(Node::Integer(i)),
            _ => None,
        }
    }

    fn reduce(
        &mut self,
        rule: Production,
        rhs: &mut Reduction<'_, Option<Node>>,
    ) -> Result<Option<Node>, Infallible> {
        if rule.info().production == "a_expr: a_expr '+' a_expr" {
            self.0 += 1;
            Ok(Some(
                AExpr {
                    lexpr: Some(Box::new(rhs.take(1).unwrap())),
                    rexpr: Some(Box::new(rhs.take(3).unwrap())),
                    ..Default::default()
                }
                .into(),
            ))
        } else {
            Ok(rhs.default_value())
        }
    }
}

#[test]
fn syntax_failure_drops_deep_live_semantic_values() {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let sql = format!("SELECT 1{}+)", "+1".repeat(20_000));
            let mut actions = Additions(0);
            let result = parse(&sql, ParseMode::Sql, Options::default(), &mut actions);
            assert_eq!(actions.0, 20_000);
            assert!(matches!(result, Err(ParseError::Diagnostic(e)) if e.position == sql.len()));
        })
        .unwrap()
        .join()
        .unwrap();
}
