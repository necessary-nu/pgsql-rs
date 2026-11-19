//! Shift/reduce driver for the patched PostgreSQL 19 grammar.
#![forbid(unsafe_code)]

use pg19_lexer::{Error, Kind, Options, Scanner, Token, Value};
use pg19_nodes::Location;
use pg19_tables::{action, goto, Action, Production, State, INITIAL_STATE};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ParseMode {
    #[default]
    Sql,
    TypeName,
    PlPgSqlExpr,
    PlPgSqlAssign1,
    PlPgSqlAssign2,
    PlPgSqlAssign3,
}

impl ParseMode {
    fn prefix(self) -> Option<Token> {
        let kind = match self {
            Self::Sql => return None,
            Self::TypeName => Kind::MODE_TYPE_NAME,
            Self::PlPgSqlExpr => Kind::MODE_PLPGSQL_EXPR,
            Self::PlPgSqlAssign1 => Kind::MODE_PLPGSQL_ASSIGN1,
            Self::PlPgSqlAssign2 => Kind::MODE_PLPGSQL_ASSIGN2,
            Self::PlPgSqlAssign3 => Kind::MODE_PLPGSQL_ASSIGN3,
        };
        Some(Token {
            kind,
            start: 0,
            end: 0,
            keyword: None,
            value: Value::None,
        })
    }
}

pub trait Actions {
    type Value: Default;
    type Error;

    fn shift(&mut self, token: Token) -> Self::Value;

    /// Called for productions with explicit actions, including an explicit `$$ = $1`.
    /// The result for `parse_toplevel` is returned once Bison reaches its final state.
    fn reduce(
        &mut self,
        rule: Production,
        rhs: &mut Reduction<'_, Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum ParseError<E> {
    Diagnostic(Error),
    Action(E),
}

impl<E: std::fmt::Display> std::fmt::Display for ParseError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Diagnostic(error) => error.fmt(f),
            Self::Action(error) => error.fmt(f),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for ParseError<E> {}

struct Frame<V> {
    state: State,
    value: V,
    location: Location,
}

pub struct Reduction<'a, V> {
    stack: &'a mut [Frame<V>],
    start: usize,
    scanner_span: (usize, usize),
    /// The initial `@$` is the first known position on the RHS or -1.
    pub lhs_location: Location,
}

impl<V: Default> Reduction<'_, V> {
    fn index(&self, n: isize) -> usize {
        // $1 begins the RHS. In a midrule action $0 is the preceding stack entry.
        (self.start - 1)
            .checked_add_signed(n)
            .expect("reference before the parse stack")
    }

    /// Half-open byte span of the last token fetched by the parser, including
    /// pending lookahead. At EOF both offsets are the end of the SQL text.
    pub fn scanner_span(&self) -> (usize, usize) {
        self.scanner_span
    }

    pub fn value(&self, n: isize) -> &V {
        &self.stack[self.index(n)].value
    }

    pub fn location(&self, n: isize) -> Location {
        self.stack[self.index(n)].location
    }

    /// Move an owned RHS value without cloning a subtree or its list.
    pub fn take(&mut self, n: isize) -> V {
        let index = self.index(n);
        std::mem::take(&mut self.stack[index].value)
    }

    pub fn default_value(&mut self) -> V {
        if self.start == self.stack.len() {
            // Bison leaves the value of a default empty production unspecified.
            V::default()
        } else {
            self.take(1)
        }
    }
}

fn reduce<A: Actions>(
    stack: &mut Vec<Frame<A::Value>>,
    rule: Production,
    actions: &mut A,
    scanner_span: (usize, usize),
) -> Result<(), A::Error> {
    let start = stack.len() - rule.rhs_len();
    let state = goto(stack[start - 1].state, rule);
    let location = stack[start..]
        .iter()
        .map(|frame| frame.location)
        .find(|location| *location != Location::UNKNOWN)
        .unwrap_or(Location::UNKNOWN);
    let mut rhs = Reduction {
        stack,
        start,
        scanner_span,
        lhs_location: location,
    };
    let value = if rule.info().action.is_some() {
        actions.reduce(rule, &mut rhs)?
    } else {
        rhs.default_value()
    };
    let location = rhs.lhs_location;
    stack.truncate(start);
    stack.push(Frame {
        state,
        value,
        location,
    });
    Ok(())
}

enum Input {
    Token(Token),
    Eof,
}

impl Input {
    fn id(&self) -> u16 {
        match self {
            Self::Token(token) => token.kind.id(),
            Self::Eof => 0,
        }
    }
}

// [spec:pgq:req:grammar.reductions]
pub fn parse<A: Actions>(
    sql: &str,
    mode: ParseMode,
    options: Options,
    actions: &mut A,
) -> Result<A::Value, ParseError<A::Error>> {
    let sql = sql.split_once('\0').map_or(sql, |(prefix, _)| prefix);
    if Location::from_byte_offset(sql.len()).is_none() {
        return Err(ParseError::Diagnostic(Error::without_position(
            "SQL input exceeds PostgreSQL's location range",
            "54000",
        )));
    }
    let mut scanner = mode
        .prefix()
        .into_iter()
        .map(Ok)
        .chain(Scanner::with_options(sql, options).grammar());
    let mut stack = vec![Frame {
        state: INITIAL_STATE,
        value: A::Value::default(),
        location: Location::default(),
    }];
    let mut lookahead = None;
    let mut span = (0, 0);
    loop {
        // Bison's generated skeleton checks YYMAXDEPTH on entry to each state.
        if stack.len() >= 10_000 {
            return Err(ParseError::Diagnostic(Error::near(
                sql,
                span.0,
                span.1,
                "memory exhausted",
            )));
        }
        let state = stack.last().unwrap().state;
        match action(state, lookahead.as_ref().map(Input::id)) {
            Action::NeedToken => {
                let token = scanner.next().transpose().map_err(ParseError::Diagnostic)?;
                span = token
                    .as_ref()
                    .map_or((sql.len(), sql.len()), |t| (t.start, t.end));
                lookahead = Some(token.map_or(Input::Eof, Input::Token));
            }
            Action::Shift(state) => {
                let value = match lookahead.take().expect("shift requires lookahead") {
                    Input::Token(token) => actions.shift(token),
                    Input::Eof => A::Value::default(),
                };
                stack.push(Frame {
                    state,
                    value,
                    location: Location::from_byte_offset(span.0).unwrap(),
                });
            }
            Action::Reduce(rule) => {
                reduce(&mut stack, rule, actions, span).map_err(ParseError::Action)?
            }
            Action::Accept => {
                stack.pop(); // The accepting state is reached by shifting EOF after parse_toplevel.
                return Ok(stack.pop().unwrap().value);
            }
            Action::Error => {
                return Err(ParseError::Diagnostic(Error::near(
                    sql,
                    span.0,
                    span.1,
                    "syntax error",
                )))
            }
        }
    }
}
