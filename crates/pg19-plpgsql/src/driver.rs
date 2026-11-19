use crate::{BodyScanner, Error, Namespace, Options, Production, Token};
use pg19_tables::plpgsql::{action, goto, Action, State, INITIAL_STATE};

pub struct Input<'a> {
    pub scanner: BodyScanner<'a>,
    /// A datatype reduction can consume Bison's pending token (yychar) and clear it.
    pub lookahead: Option<Token>,
}

pub trait Actions: Namespace {
    type Value: Default;
    type Error: From<Error>;

    fn shift(&mut self, token: Token) -> Self::Value;
    fn reduce(
        &mut self,
        rule: Production,
        rhs: &mut Reduction<'_, Self::Value>,
        input: &mut Input<'_>,
    ) -> Result<Self::Value, Self::Error>;
}

struct Frame<V> {
    state: State,
    location: usize,
    value: V,
}

pub struct Reduction<'a, V> {
    frames: &'a mut [Frame<V>],
    start: usize,
    pub location: usize,
}

impl<V: Default> Reduction<'_, V> {
    fn index(&self, n: isize) -> usize {
        (self.start - 1)
            .checked_add_signed(n)
            .expect("reference before the parse stack")
    }

    pub fn value(&self, n: isize) -> &V {
        &self.frames[self.index(n)].value
    }

    pub fn location(&self, n: isize) -> usize {
        self.frames[self.index(n)].location
    }

    pub fn take(&mut self, n: isize) -> V {
        let index = self.index(n);
        std::mem::take(&mut self.frames[index].value)
    }

    pub fn default_value(&mut self) -> V {
        if self.start == self.frames.len() {
            V::default()
        } else {
            self.take(1)
        }
    }
}

pub fn parse_body<A: Actions>(
    source: &str,
    options: Options,
    actions: &mut A,
) -> Result<A::Value, A::Error> {
    let mut input = Input {
        scanner: BodyScanner::new(source, options),
        lookahead: None,
    };
    drive(&mut input, actions)
}

pub(crate) fn drive<A: Actions>(input: &mut Input, actions: &mut A) -> Result<A::Value, A::Error> {
    let mut stack = vec![Frame {
        state: INITIAL_STATE,
        location: 0,
        value: A::Value::default(),
    }];
    loop {
        if stack.len() >= 10_000 {
            return Err(input.scanner.error("memory exhausted").into());
        }
        let state = stack.last().unwrap().state;
        match action(state, input.lookahead.as_ref().map(|t| t.id)) {
            Action::NeedToken => input.lookahead = Some(input.scanner.next_token(actions)?),
            Action::Shift(state) => {
                let token = input.lookahead.take().expect("shift requires lookahead");
                let location = token.span.start;
                stack.push(Frame {
                    state,
                    location,
                    value: actions.shift(token),
                });
            }
            Action::Reduce(rule) => {
                let start = stack.len() - rule.rhs_len();
                let previous = stack[start - 1].state;
                // pl_gram.y gives an empty RHS the location of its left neighbor.
                let location = stack.get(start).unwrap_or(&stack[start - 1]).location;
                let mut rhs = Reduction {
                    frames: &mut stack,
                    start,
                    location,
                };
                let value = if rule.info().action.is_some() {
                    actions.reduce(rule, &mut rhs, input)?
                } else {
                    rhs.default_value()
                };
                let location = rhs.location;
                stack.truncate(start);
                stack.push(Frame {
                    state: goto(previous, rule),
                    location,
                    value,
                });
            }
            Action::Accept => {
                stack.pop();
                return Ok(stack.pop().unwrap().value);
            }
            Action::Error => return Err(input.scanner.error("syntax error").into()),
        }
    }
}
