//! Native semantic actions for the patched PostgreSQL 19 grammar.
#![forbid(unsafe_code)]
mod call;
mod dml;
mod expr;
mod from;
mod limits;
mod location;
mod predicate;
mod query;
mod records;
mod types;
mod utility;
mod value;

pub use pg19_engine::ParseMode;
use pg19_engine::{Actions, ParseError, Reduction};
pub use pg19_lexer::Options;
use pg19_lexer::Token;
use pg19_nodes::{Node, RawStmt};
use pg19_tables::{Production, RuleInfo};
use value::Value;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Diagnostic(pg19_lexer::Error),
    InvalidOutput(&'static str),
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
            Self::InvalidOutput(message) => f.write_str(message),
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
    At(String, &'static str, pg19_nodes::Location),
}
impl From<&'static str> for Failure {
    fn from(s: &'static str) -> Self {
        Self::Value(s)
    }
}

enum Op {
    Generated(fn(&mut Reduction<'_, Value>) -> Result<Value, Failure>),
    Take(isize),
    Integer(i32),
    Call(call::Call),
    Limit(limits::Limit),
    Types(types::Types),
    From(from::From),
    Predicate(predicate::Predicate),
    Dml(dml::Dml),
    Empty,
    Nil,
    Single,
    Append(isize),
    RuleActions { first: bool },
    Utility(utility::Utility),
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
                Op::Generated(handler) => handler(rhs)?,
                Op::Take(i) => rhs.take(i),
                Op::Integer(v) => Value::Integer(v),
                Op::Types(op) => types::reduce(op, rhs)?,
                Op::Limit(op) => limits::reduce(op, rhs)?,
                Op::Call(op) => call::reduce(op, rhs)?,
                Op::Dml(op) => dml::reduce(op, rhs)?,
                Op::Predicate(op) => predicate::reduce(op, rhs)?,
                Op::From(op) => from::reduce(op, rhs)?,
                Op::Empty => Value::Empty,
                Op::Nil => Value::List(Vec::new()),
                Op::Single => Value::List(vec![rhs.take(1).node()?]),
                Op::RuleActions { first } => {
                    let (mut list, next) = if first {
                        (Vec::new(), rhs.take(1))
                    } else {
                        (rhs.take(1).list()?, rhs.take(3))
                    };
                    if let Some(stmt) = next.optional()? {
                        list.push(*stmt);
                    }
                    Value::List(list)
                }
                Op::Append(i) => {
                    let mut items = rhs.take(1).list()?;
                    items.push(rhs.take(i).node()?);
                    Value::List(items)
                }
                Op::Utility(op) => utility::reduce(op, rhs)?,
                Op::Expr(e) => expr::reduce(e, rhs)?,
                Op::Query(q) => query::reduce(q, rhs)?,
            })
        })();
        result.map_err(|failure| match failure {
            Failure::Value(expected) => Error::InvalidValue {
                rule: rule.info(),
                expected,
            },
            Failure::At(message, sqlstate, location) => {
                Error::Diagnostic(match location.byte_offset() {
                    Some(offset) => pg19_lexer::Error::at(self.sql, offset, message, sqlstate),
                    None => pg19_lexer::Error::without_position(message, sqlstate),
                })
            }
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
        Ok(_) => Err(Error::InvalidOutput("start action did not return a list")),
        Err(ParseError::Diagnostic(e)) => Err(Error::Diagnostic(e)),
        Err(ParseError::Action(e)) => Err(e),
    }
}

pub fn parse_statements(sql: &str, options: Options) -> Result<Vec<RawStmt>, Error> {
    parse(sql, ParseMode::Sql, options)?
        .into_iter()
        .map(|node| match node {
            Node::RawStmt(stmt) => Ok(stmt.into_inner()),
            _ => Err(Error::InvalidOutput(
                "default parse mode returned a non-statement",
            )),
        })
        .collect()
}
