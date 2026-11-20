//! Native semantic actions for the patched PostgreSQL 19 grammar.
#![deny(unsafe_code)]
macro_rules! vec {
    ($($item:expr),* $(,)?) => { crate::raw::Vec::from([$($item),*]) };
}
mod builtin;
mod call;
mod dml;
mod expr;
mod from;
mod json;
mod limits;
mod location;
mod predicate;
mod query;
mod raw;
mod records;
mod types;
mod utility;
mod value;
mod window;
mod xmltable;

pub use pg19_engine::ParseMode;
use pg19_engine::{ParseError, ParseStack, Reduction, StackActions};
pub use pg19_lexer::Options;
use pg19_lexer::Token;
use pg19_nodes::{Node, RawStmt};
use pg19_tables::{Production, RuleInfo, State};
use raw::Vec as ArenaVec;
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
    #[cold]
    fn from(s: &'static str) -> Self {
        Self::Value(s)
    }
}

#[cold]
#[inline(never)]
fn reduction_error(number: u16, failure: Failure) -> Box<(Production, Failure)> {
    let rule = Production::from_number(number).expect("generated action has a production");
    Box::new((rule, failure))
}

enum Op {
    Generated(fn(&mut Reduction<'_, Value>) -> Result<Value, Failure>),
    Take(isize),
    Integer(i32),
    Call(call::Call),
    Limit(limits::Limit),
    Types(types::Types),
    From(from::From),
    Json(json::Json),
    Builtin(builtin::Builtin),
    Window(window::Window),
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

type Handler =
    fn(&mut ParseStack<Value>, State, (usize, usize)) -> Result<State, Box<(Production, Failure)>>;

struct SqlEnd {
    list: State,
    before_eof: State,
    accept: State,
    append: Production,
    semicolon: u16,
}

impl SqlEnd {
    fn finish(
        &self,
        stack: &mut ParseStack<Value>,
        span: (usize, usize),
    ) -> Result<State, Box<(Production, Failure)>> {
        let Some(token) = stack.lookahead() else {
            return Ok(self.list);
        };
        let separator = token.symbol() == self.semicolon;
        if !separator && token.symbol() != 0 {
            return Ok(self.list);
        }
        let finished = stack
            .finish_root(span.1, self.before_eof, |value| {
                if separator {
                    let Value::Statements(statements) = value else {
                        return Err(Failure::Value("expected statements"));
                    };
                    query::terminate(
                        statements,
                        pg19_nodes::Location::from_byte_offset(span.0).unwrap(),
                    )?;
                }
                Ok(())
            })
            .map_err(|failure| reduction_error(self.append.number(), failure))?;
        Ok(if finished { self.accept } else { self.list })
    }
}

#[cfg_attr(not(debug_assertions), inline(always))]
fn apply(op: Op, rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    Ok(match op {
        Op::Generated(handler) => handler(rhs)?,
        Op::Take(i) => rhs.take(i),
        Op::Integer(v) => Value::Integer(v),
        Op::Types(op) => types::reduce(op, rhs)?,
        Op::Limit(op) => limits::reduce(op, rhs)?,
        Op::Call(op) => call::reduce(op, rhs)?,
        Op::Dml(op) => dml::reduce(op, rhs)?,
        Op::Predicate(op) => predicate::reduce(op, rhs)?,
        Op::Window(op) => window::reduce(op, rhs)?,
        Op::Builtin(op) => builtin::reduce(op, rhs)?,
        Op::Json(op) => json::reduce(op, rhs)?,
        Op::From(op) => from::reduce(op, rhs)?,
        Op::Empty => Value::Empty,
        Op::Nil => Value::from_list(ArenaVec::new()),
        Op::Single => Value::from_list(vec![rhs.take(1).node()?]),
        Op::RuleActions { first } => {
            let (mut list, next) = if first {
                (ArenaVec::new(), rhs.take(1))
            } else {
                (rhs.take(1).list()?, rhs.take(3))
            };
            if let Some(stmt) = next.optional()? {
                list.push(stmt.into_inner());
            }
            Value::from_list(list)
        }
        Op::Append(i) => {
            let mut items = rhs.take(1).list()?;
            items.push(rhs.take(i).node()?);
            Value::from_list(items)
        }
        Op::Utility(op) => utility::reduce(op, rhs)?,
        Op::Expr(e) => expr::reduce(e, rhs)?,
        Op::Query(q) => query::reduce(q, rhs)?,
    })
}

struct Parser<'a> {
    sql: &'a str,
}

// [spec:pgq:req:grammar.overlay-differences]
impl StackActions<raw::String> for Parser<'_> {
    type Value = Value;
    type Error = Box<Error>;

    const COMPILED_SHIFTS: Option<&'static [u8]> =
        Some(include_bytes!(concat!(env!("OUT_DIR"), "/shifts.bin")));

    #[inline]
    fn shift_literal(
        &mut self,
        token: &mut Token<raw::String>,
        location: pg19_nodes::Location,
    ) -> Value {
        use pg19_lexer::{Kind, Value as Lexeme};
        use raw::{AConst, Literal};
        let lexeme = std::mem::replace(&mut token.value, Lexeme::None);
        let val = match (token.kind, lexeme) {
            (Kind::ICONST, Lexeme::Integer(value)) => Literal::Integer(value),
            (Kind::FCONST, Lexeme::Text(text)) => Literal::Float(text),
            (Kind::SCONST, Lexeme::Text(text)) => Literal::String(text),
            (Kind::BCONST | Kind::XCONST, Lexeme::Text(text)) => Literal::BitString(text),
            (Kind::TRUE_P, _) => Literal::Boolean(true),
            (Kind::FALSE_P, _) => Literal::Boolean(false),
            (Kind::NULL_P, _) => Literal::Null,
            _ => unreachable!("folded literal does not match its lexer token"),
        };
        Value::from_node(AConst { val, location }.into())
    }

    #[inline]
    fn shift(&mut self, token: &mut Token<raw::String>) -> Value {
        match std::mem::replace(&mut token.value, pg19_lexer::Value::None) {
            pg19_lexer::Value::None => Value::Empty,
            pg19_lexer::Value::Integer(n) => Value::Integer(n),
            pg19_lexer::Value::Text(s) => Value::from_text(s),
            pg19_lexer::Value::Keyword(s) => Value::Keyword(s),
        }
    }

    #[cfg_attr(not(debug_assertions), inline(always))]
    fn reduce_stack(
        &mut self,
        rule: Production,
        stack: &mut ParseStack<Value>,
        current: State,
        span: (usize, usize),
    ) -> Result<State, Box<Error>> {
        let handler =
            HANDLERS[usize::from(rule.number())].ok_or_else(|| Error::Unmapped(rule.info()))?;
        handler(stack, current, span).map_err(|failed| {
            let (rule, failure) = *failed;
            Box::new(self.error(rule, span, failure))
        })
    }
}

impl Parser<'_> {
    #[cold]
    fn error(&self, rule: Production, span: (usize, usize), failure: Failure) -> Error {
        match failure {
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
                let (a, b) = span;
                let sql = self.sql.split('\0').next().unwrap();
                Error::Diagnostic(pg19_lexer::Error::near(sql, a, b, message))
            }
        }
    }
}

/// Parse a PostgreSQL 19 raw grammar start mode. SQL mode returns ordered RawStmt nodes.
// [spec:pgq:req:grammar.raw-sql]
// [spec:pgq:req:grammar.sql-families]
// [spec:pgq:req:grammar.parse-modes]
// [spec:pgq:req:grammar.runtime]
pub fn parse(sql: &str, mode: ParseMode, options: Options) -> Result<Vec<Node>, Error> {
    if mode == ParseMode::Sql {
        return Ok(parse_raw(sql, options)?
            .into_owned()
            .into_iter()
            .map(Node::from)
            .collect());
    }
    let region = raw::Region::new();
    region.build(|| match parse_value(sql, mode, options)? {
        Value::List(nodes) => Ok(nodes.iter().map(raw::materialize).collect()),
        _ => Err(Error::InvalidOutput("start action did not return a list")),
    })
}

fn parse_value(sql: &str, mode: ParseMode, options: Options) -> Result<Value, Error> {
    thread_local! {
        static SCRATCH: std::cell::Cell<ParseStack<Value>> = const { std::cell::Cell::new(ParseStack::new()) };
    }
    SCRATCH
        .with(|cache| {
            let mut stack = cache.take();
            let result =
                pg19_engine::parse_with_text(sql, mode, options, &mut Parser { sql }, &mut stack);
            cache.set(stack);
            result
        })
        .map_err(|err| match err {
            ParseError::Diagnostic(e) => Error::Diagnostic(e),
            ParseError::Action(e) => *e,
        })
}

pub fn parse_statements(sql: &str, options: Options) -> Result<Vec<RawStmt>, Error> {
    parse_raw(sql, options).map(RawTree::into_owned)
}

/// Complete raw SQL tree. Its nodes and text live until the result is dropped.
pub struct RawTree {
    statements: raw::StatementList,
    _region: raw::Region,
}
pub use raw::json::EncodeError;

/// Read-only statement in a raw result.
#[derive(Clone, Copy)]
pub struct RawStatement<'tree>(&'tree raw::RawStmt);
impl RawStatement<'_> {
    /// Copy this statement and its descendants into independently mutable nodes.
    pub fn to_owned(self) -> RawStmt {
        raw::owned_statement(self.0)
    }

    pub fn tag(&self) -> Option<pg19_nodes::NodeTag> {
        self.0.stmt.as_deref()?.tag()
    }
    pub fn location(&self) -> pg19_nodes::Location {
        self.0.stmt_location
    }
    pub fn source_range(&self, source_len: usize) -> Option<std::ops::Range<usize>> {
        self.0.source_range(source_len)
    }
    pub fn json(&self) -> Result<String, EncodeError> {
        match self.0.stmt.as_deref() {
            Some(n) => raw::json::node_json(n),
            None => Ok("null".into()),
        }
    }
}
impl std::fmt::Debug for RawStatement<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawStatement")
            .field("tag", &self.tag())
            .field("location", &self.location())
            .finish()
    }
}
impl std::fmt::Debug for RawTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.statements()).finish()
    }
}
impl RawTree {
    pub fn statements(&self) -> impl ExactSizeIterator<Item = RawStatement<'_>> {
        self.statements.as_slice().iter().map(RawStatement)
    }
    pub fn json(&self) -> Result<String, EncodeError> {
        raw::json::statements_json(self.statements.as_slice())
    }

    pub fn len(&self) -> usize {
        self.statements.as_slice().len()
    }
    pub fn is_empty(&self) -> bool {
        self.statements.as_slice().is_empty()
    }
    /// Materialize independent PostgreSQL records and release the arena.
    pub fn into_owned(self) -> Vec<RawStmt> {
        self.statements
            .into_statements()
            .iter()
            .map(raw::owned_statement)
            .collect()
    }
}
/// Parse directly into a per-result arena.
// [spec:pgq:req:grammar.parse-speed+1]
pub fn parse_raw(sql: &str, options: Options) -> Result<RawTree, Error> {
    let region = raw::Region::new();
    let statements = region.build(|| match parse_value(sql, ParseMode::Sql, options)? {
        Value::Statements(s) => Ok(s),
        _ => Err(Error::InvalidOutput(
            "default parse mode returned a non-statement",
        )),
    })?;
    Ok(RawTree {
        statements,
        _region: region,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Use grammar block boundaries so a new or renamed production requires
    // attention even if the PG regression corpus does not reduce it yet.
    fn check_registered_block(first: &str, after: &str) {
        let mut inside = false;
        let mut checks = 0;
        for rule in pg19_tables::productions() {
            let info = rule.info();
            let (lhs, _) = info.production.split_once(':').unwrap();
            if inside && lhs == after {
                assert!(checks > 0);
                return;
            }
            inside |= lhs == first;
            if inside && info.action.is_some() {
                assert!(
                    HANDLERS[usize::from(rule.number())].is_some(),
                    "missing {}",
                    info.production
                );
                checks += 1;
            }
        }
        panic!("grammar block boundary missing for {first} to {after}");
    }

    #[test]
    fn query_and_expression_actions_cover_their_grammar_blocks() {
        check_registered_block("SelectStmt", "Typename");
        check_registered_block("Typename", "ColId");
    }

    #[test]
    fn dml_actions_cover_their_grammar_block() {
        check_registered_block("InsertStmt", "SelectStmt");
    }

    #[test]
    fn every_explicit_pg19_action_is_registered() {
        for rule in pg19_tables::productions() {
            let info = rule.info();
            if info.action.is_some() {
                assert!(
                    HANDLERS[usize::from(rule.number())].is_some(),
                    "unmapped PostgreSQL action at gram.y:{} for {}",
                    info.line,
                    info.production
                );
            }
        }
    }
}
