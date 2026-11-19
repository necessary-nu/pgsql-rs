//! Native semantic actions for the patched PostgreSQL 19 grammar.
#![forbid(unsafe_code)]
mod expr;
mod query;
mod value;

use pg19_engine::{Actions, ParseError, ParseMode, Reduction};
use pg19_lexer::{Options, Token};
use pg19_nodes::Node;
use pg19_tables::{Production, RuleInfo};
use value::Value;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Diagnostic(pg19_lexer::Error),
    Unmapped(RuleInfo),
    InvalidValue {
        rule: RuleInfo,
        expected: &'static str,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Diagnostic(e) => e.fmt(f),
            Self::Unmapped(rule) => write!(
                f,
                "unmapped PostgreSQL action at gram.y:{} for {}",
                rule.line, rule.production
            ),
            Self::InvalidValue { rule, expected } => {
                write!(f, "{expected} during {}", rule.production)
            }
        }
    }
}
impl std::error::Error for Error {}

pub(crate) enum Failure {
    Value(&'static str),
    Syntax(&'static str),
}
impl From<&'static str> for Failure {
    fn from(s: &'static str) -> Self {
        Self::Value(s)
    }
}

enum Op {
    Take(isize),
    Empty,
    Nil,
    Single,
    Append(isize),
    Expr(expr::Expr),
    Query(query::Query),
}
include!(concat!(env!("OUT_DIR"), "/actions.rs"));

struct Parser<'a> {
    sql: &'a str,
}

impl Actions for Parser<'_> {
    type Value = Value;
    type Error = Error;

    fn shift(&mut self, token: Token) -> Value {
        match token.value {
            pg19_lexer::Value::None => Value::Empty,
            pg19_lexer::Value::Integer(n) => Value::Integer(n),
            pg19_lexer::Value::Text(s) => Value::Text(s),
            pg19_lexer::Value::Keyword(s) => Value::Text(s.to_owned()),
        }
    }

    fn reduce(&mut self, rule: Production, rhs: &mut Reduction<'_, Value>) -> Result<Value, Error> {
        let op = action(rule.number()).ok_or_else(|| Error::Unmapped(rule.info()))?;
        let result = (|| {
            Ok(match op {
                Op::Take(i) => rhs.take(i),
                Op::Empty => Value::Empty,
                Op::Nil => Value::List(Vec::new()),
                Op::Single => Value::List(vec![rhs.take(1).node()?]),
                Op::Append(i) => {
                    let mut items = rhs.take(1).list()?;
                    items.push(rhs.take(i).node()?);
                    Value::List(items)
                }
                Op::Expr(e) => expr::reduce(e, rhs)?,
                Op::Query(q) => query::reduce(q, rhs)?,
            })
        })();
        result.map_err(|failure| match failure {
            Failure::Value(expected) => Error::InvalidValue {
                rule: rule.info(),
                expected,
            },
            Failure::Syntax(message) => {
                let (a, b) = rhs.scanner_span();
                Error::Diagnostic(pg19_lexer::Error::near(self.sql, a, b, message))
            }
        })
    }
}

/// Parse a raw grammar start mode. Supported SQL returns PostgreSQL's list
/// of RawStmt nodes. An unported production reports its upstream identity.
pub fn parse(sql: &str, mode: ParseMode, options: Options) -> Result<Vec<Node>, Error> {
    let sql = sql.split_once('\0').map_or(sql, |(prefix, _)| prefix);
    let mut parser = Parser { sql };
    match pg19_engine::parse(sql, mode, options, &mut parser) {
        Ok(Value::List(nodes)) => Ok(nodes),
        Ok(_) => unreachable!("every start-rule action returns its raw parse tree list"),
        Err(ParseError::Diagnostic(e)) => Err(Error::Diagnostic(e)),
        Err(ParseError::Action(e)) => Err(e),
    }
}
