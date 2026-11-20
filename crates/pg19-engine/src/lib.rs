//! Shift/reduce driver for the patched PostgreSQL 19 grammar.
#![forbid(unsafe_code)]

use pg19_lexer::{Error, Kind, Options, Scanner, Token, Value};
use pg19_nodes::Location;
use pg19_tables::{goto, step, Action, Lookahead, Production, State, INITIAL_STATE};

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
    previous: State,
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
        (self.start - 1).wrapping_add_signed(n)
    }

    /// Half-open byte span of the last token fetched by the parser, including
    /// pending lookahead. At EOF both offsets are the end of the SQL text.
    pub fn scanner_span(&self) -> (usize, usize) {
        self.scanner_span
    }

    pub fn value(&self, n: isize) -> &V {
        &self.stack[self.index(n)].value
    }

    #[inline]
    pub fn location(&self, n: isize) -> Location {
        self.stack[self.index(n)].location
    }

    /// Move an owned RHS value without cloning a subtree or its list.
    #[inline]
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

/// The LR stack available to generated semantic actions. Its operations apply
/// the same Bison location, goto and ownership rules as the callback driver.
pub struct ParseStack<V> {
    frames: Vec<Frame<V>>,
    lookahead: Option<Lookahead>,
    input_len: usize,
}

const MAX_DEPTH: usize = 10_000;

impl<V> Default for ParseStack<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> ParseStack<V> {
    pub const fn new() -> Self {
        Self {
            frames: Vec::new(),
            lookahead: None,
            input_len: 0,
        }
    }

    /// The current lookahead, if Bison has already requested a token.
    pub fn lookahead(&self) -> Option<Lookahead> {
        self.lookahead
    }

    /// Every intermediate state must stay below YYMAXDEPTH when combining reductions.
    pub fn has_room(&self, additional: usize) -> bool {
        self.frames.len().saturating_add(additional) < MAX_DEPTH
    }

    /// Release all semantic values. Stacks larger than 256 cells also release their buffer.
    pub fn clear(&mut self) {
        self.lookahead = None;
        if self.frames.capacity() > 256 {
            self.frames = Vec::new();
        } else {
            self.frames.clear();
        }
    }
}

impl<V: Default> ParseStack<V> {
    /// Complete a reduced start symbol when its last token reaches the end of
    /// the scanner's input. The accept action expects an EOF frame above it.
    #[inline]
    pub fn finish_root<E>(
        &mut self,
        end: usize,
        before_eof: State,
        update: impl FnOnce(&mut V) -> Result<(), E>,
    ) -> Result<bool, E> {
        if end != self.input_len || self.frames.len() != 2 {
            return Ok(false);
        }
        update(&mut self.frames[1].value)?;
        self.frames.push(Frame {
            previous: before_eof,
            value: V::default(),
            location: Location::from_byte_offset(end).unwrap(),
        });
        Ok(true)
    }

    /// Apply an unconditional unit action. Its only stack cell stays in place.
    #[cfg_attr(not(debug_assertions), inline(always))]
    pub fn map_top<E>(
        &mut self,
        span: (usize, usize),
        action: impl FnOnce(&mut Reduction<'_, V>) -> Result<V, E>,
    ) -> Result<(), E> {
        let stack = &mut self.frames;
        if stack.len() >= MAX_DEPTH {
            return Ok(());
        }
        let start = stack.len() - 1;
        let location = stack[start].location;
        let mut rhs = Reduction {
            stack,
            start,
            scanner_span: span,
            lhs_location: location,
        };
        let value = action(&mut rhs)?;
        let location = rhs.lhs_location;
        stack[start].value = value;
        stack[start].location = location;
        Ok(())
    }

    #[cfg_attr(not(debug_assertions), inline(always))]
    pub fn reduce<E>(
        &mut self,
        rule: Production,
        current: State,
        span: (usize, usize),
        next_state: impl FnOnce(State) -> State,
        action: impl FnOnce(&mut Reduction<'_, V>) -> Result<V, E>,
    ) -> Result<State, E> {
        self.reduce_prefix(rule.rhs_len(), current, span, next_state, action)
    }

    /// Reduce the materialized prefix when the remaining RHS symbols all derive
    /// the empty string. Their unknown locations do not affect the LHS location.
    #[cfg_attr(not(debug_assertions), inline(always))]
    pub fn reduce_prefix<E>(
        &mut self,
        length: usize,
        current: State,
        span: (usize, usize),
        next_state: impl FnOnce(State) -> State,
        action: impl FnOnce(&mut Reduction<'_, V>) -> Result<V, E>,
    ) -> Result<State, E> {
        let stack = &mut self.frames;
        let start = stack.len() - length;
        let previous = if length == 0 {
            current
        } else {
            stack[start].previous
        };
        let state = next_state(previous);
        let location = stack[start..]
            .iter()
            .map(|f| f.location)
            .find(|l| *l != Location::UNKNOWN)
            .unwrap_or(Location::UNKNOWN);
        let mut rhs = Reduction {
            stack,
            start,
            scanner_span: span,
            lhs_location: location,
        };
        let value = action(&mut rhs)?;
        let frame = Frame {
            previous,
            location: rhs.lhs_location,
            value,
        };
        if length == 0 {
            stack.push(frame);
        } else {
            stack.truncate(start + 1);
            stack[start] = frame;
        }
        Ok(state)
    }

    #[cfg_attr(not(debug_assertions), inline(always))]
    pub fn reduce_default(
        &mut self,
        rule: Production,
        current: State,
        next_state: impl FnOnce(State) -> State,
    ) -> State {
        let stack = &mut self.frames;
        let length = rule.rhs_len();
        let start = stack.len() - length;
        let previous = if length == 0 {
            current
        } else {
            stack[start].previous
        };
        let state = next_state(previous);
        if length == 0 {
            stack.push(Frame {
                previous,
                location: Location::UNKNOWN,
                value: V::default(),
            });
        } else if length > 1 {
            stack[start].location = stack[start..]
                .iter()
                .map(|f| f.location)
                .find(|l| *l != Location::UNKNOWN)
                .unwrap_or(Location::UNKNOWN);
            stack.truncate(start + 1);
        }
        state
    }
}

/// A dispatcher may fold unconditional unit reductions before returning the next state.
pub trait StackActions {
    type Value: Default;
    type Error;

    fn shift(&mut self, token: &mut Token) -> Self::Value;
    fn reduce_stack(
        &mut self,
        rule: Production,
        stack: &mut ParseStack<Self::Value>,
        current: State,
        span: (usize, usize),
    ) -> Result<State, Self::Error>;
}

impl<A: Actions> StackActions for A {
    type Value = A::Value;
    type Error = A::Error;
    fn shift(&mut self, token: &mut Token) -> Self::Value {
        Actions::shift(self, std::mem::take(token))
    }
    fn reduce_stack(
        &mut self,
        rule: Production,
        stack: &mut ParseStack<Self::Value>,
        current: State,
        span: (usize, usize),
    ) -> Result<State, Self::Error> {
        if !rule.has_action() {
            Ok(stack.reduce_default(rule, current, |state| goto(state, rule)))
        } else {
            stack.reduce(
                rule,
                current,
                span,
                |state| goto(state, rule),
                |rhs| self.reduce(rule, rhs),
            )
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
    parse_with(sql, mode, options, actions, &mut ParseStack::default())
}

pub fn parse_with<A: StackActions>(
    sql: &str,
    mode: ParseMode,
    options: Options,
    context: &mut A,
    stack: &mut ParseStack<A::Value>,
) -> Result<A::Value, ParseError<A::Error>> {
    stack.clear();
    let result = run(sql, mode, options, context, stack);
    stack.clear();
    result
}

fn run<A: StackActions>(
    sql: &str,
    mode: ParseMode,
    options: Options,
    context: &mut A,
    stack: &mut ParseStack<A::Value>,
) -> Result<A::Value, ParseError<A::Error>> {
    let scanner = Scanner::with_options(sql, options);
    let sql = scanner.source();
    if Location::from_byte_offset(sql.len()).is_none() {
        return Err(ParseError::Diagnostic(Error::without_position(
            "SQL input exceeds PostgreSQL's location range",
            "54000",
        )));
    }
    let mut scanner = scanner.grammar();
    stack.input_len = sql.len();
    stack.frames.reserve(32);
    stack.frames.push(Frame {
        previous: INITIAL_STATE,
        value: A::Value::default(),
        location: Location::default(),
    });
    let prefix = mode.prefix();
    let mut symbol = prefix.as_ref().map(|token| Lookahead::new(token.kind.id()));
    let mut lookahead = prefix.unwrap_or_default();
    let mut span = (0, 0);
    let mut state = INITIAL_STATE;
    loop {
        // Bison's generated skeleton checks YYMAXDEPTH on entry to each state.
        if stack.frames.len() >= MAX_DEPTH {
            return Err(ParseError::Diagnostic(Error::near(
                sql,
                span.0,
                span.1,
                "memory exhausted",
            )));
        }
        match step(state, symbol) {
            Action::NeedToken => {
                if scanner
                    .next_into(&mut lookahead)
                    .map_err(ParseError::Diagnostic)?
                {
                    span = (lookahead.start, lookahead.end);
                } else {
                    span = (sql.len(), sql.len());
                    lookahead = Token::default();
                }
                symbol = Some(Lookahead::new(lookahead.kind.id()));
            }
            Action::Shift(next_state) => {
                let previous = state;
                state = next_state;
                symbol = None;
                let value = if lookahead.kind.id() == 0 {
                    A::Value::default()
                } else {
                    context.shift(&mut lookahead)
                };
                stack.frames.push(Frame {
                    previous,
                    value,
                    location: Location::from_byte_offset(span.0).unwrap(),
                });
            }
            Action::Reduce(rule) => {
                stack.lookahead = symbol;
                state = context
                    .reduce_stack(rule, stack, state, span)
                    .map_err(ParseError::Action)?;
            }
            Action::Accept => {
                stack.frames.pop(); // The accepting state is reached by shifting EOF after parse_toplevel.
                return Ok(stack.frames.pop().unwrap().value);
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
