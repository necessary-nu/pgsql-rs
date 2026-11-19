pub const INITIAL_STATE: State = State(0);
pub const STATE_COUNT: usize = YYNSTATES as usize;
pub const PRODUCTION_COUNT: usize = RULE_COUNT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State(u16);

impl State {
    pub fn number(self) -> u16 {
        self.0
    }

    pub const fn from_number(number: u16) -> Option<Self> {
        if (number as usize) < STATE_COUNT {
            Some(Self(number))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Production(u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    NeedToken,
    Shift(State),
    Reduce(Production),
    Accept,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActionIdentity {
    pub production: &'static str,
    pub sha256: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleInfo {
    pub production: &'static str,
    /// Line in the patched gram.y; line numbers never participate in action identity.
    pub line: usize,
    pub action: Option<ActionIdentity>,
}

fn bytes<const N: usize>(array: usize, index: usize) -> [u8; N] {
    let start = array + N * index;
    TABLES[start..start + N].try_into().unwrap()
}

fn row(text: &'static str, offsets: usize, index: usize) -> &'static str {
    let start = u32::from_le_bytes(bytes(offsets, index)) as usize;
    let end = u32::from_le_bytes(bytes(offsets, index + 1)) as usize;
    &text[start..end - 1] // exclude the inventory record's final newline
}

#[inline]
fn checked_entry(index: i32, check: u16) -> Option<i16> {
    if !(0..=YYLAST).contains(&index) {
        return None;
    }
    let slot = index as usize;
    (i16::from_le_bytes(bytes(YYCHECK, slot)) == check as i16)
        .then(|| i16::from_le_bytes(bytes(YYTABLE, slot)))
}

pub fn symbol_name(symbol: u16) -> Option<&'static str> {
    (usize::from(symbol) < SYMBOL_COUNT).then(|| row(SYMBOLS, SYMBOL_OFFSETS, usize::from(symbol)))
}

/// Translate the scanner's numeric token ID to the parser's internal symbol number.
#[inline]
pub fn terminal(external: u16) -> u16 {
    if usize::from(external) < TOKEN_COUNT {
        u16::from_le_bytes(bytes(YYTRANSLATE, usize::from(external)))
    } else {
        2 // Bison's undefined-token symbol
    }
}

/// Translated lookahead, retained across reductions until the token shifts.
#[derive(Clone, Copy, Debug)]
pub struct Lookahead(u16);

impl Lookahead {
    #[inline]
    pub fn new(external: u16) -> Self {
        // Token 256 is Bison's lexer-error signal, even in a state with a
        // default reduction after a failed lookup.
        if external == 256 {
            Self(u16::MAX)
        } else {
            Self(terminal(external))
        }
    }
}

// [spec:pgq:req:grammar.tables]
#[inline]
pub fn step(state: State, lookahead: Option<Lookahead>) -> Action {
    if state.0 == YYFINAL as u16 {
        return Action::Accept;
    }
    let index = usize::from(state.0);
    let base = i32::from_le_bytes(bytes(YYPACT, index));
    if base != YYPACT_NINF {
        let Some(Lookahead(token)) = lookahead else {
            return Action::NeedToken;
        };
        if token == u16::MAX {
            return Action::Error; // YYerror means the lexer has already reported an error.
        }
        if let Some(entry) = checked_entry(base + i32::from(token), token) {
            return match entry {
                n if n > 0 => Action::Shift(State(n as u16)),
                n if n < 0 && i32::from(n) != YYTABLE_NINF => {
                    Action::Reduce(Production((-n) as u16))
                }
                _ => Action::Error,
            };
        }
    }
    match u16::from_le_bytes(bytes(YYDEFACT, index)) {
        0 => Action::Error,
        rule => Action::Reduce(Production(rule)),
    }
}

#[inline]
pub fn action(state: State, lookahead: Option<u16>) -> Action {
    step(state, lookahead.map(Lookahead::new))
}

/// State reached after popping the RHS and pushing the reduction's LHS.
#[inline]
pub fn goto(previous: State, reduced: Production) -> State {
    let nonterminal = usize::from(reduced.lhs()) - YYNTOKENS as usize;
    let index = i32::from_le_bytes(bytes(YYPGOTO, nonterminal)) + i32::from(previous.0);
    match checked_entry(index, previous.0) {
        Some(next) => State(next as u16),
        None => State(u16::from_le_bytes(bytes(YYDEFGOTO, nonterminal))),
    }
}

impl Production {
    pub const fn from_number(number: u16) -> Option<Self> {
        if number != 0 && number as usize <= PRODUCTION_COUNT {
            Some(Self(number))
        } else {
            None
        }
    }

    #[inline]
    pub fn number(self) -> u16 {
        self.0
    }

    #[inline]
    pub fn lhs(self) -> u16 {
        u16::from_le_bytes(bytes(YYR1, usize::from(self.0)))
    }

    #[inline]
    pub fn rhs_len(self) -> usize {
        usize::from(TABLES[YYR2 + usize::from(self.0)])
    }

    #[inline]
    pub fn has_action(self) -> bool {
        TABLES[HAS_ACTION + usize::from(self.0)] != 0
    }

    // [spec:pgq:req:grammar.action-inventory]
    pub fn info(self) -> RuleInfo {
        let record = row(INVENTORY, RULE_OFFSETS, usize::from(self.0) - 1);
        let mut fields = record.split('\t');
        let production = fields.next().unwrap();
        let line = fields.next().unwrap().parse().unwrap();
        let hash = fields.next().unwrap();
        RuleInfo {
            production,
            line,
            action: (hash != "default").then_some(ActionIdentity {
                production,
                sha256: hash,
            }),
        }
    }
}

pub fn productions() -> impl Iterator<Item = Production> {
    (1..=PRODUCTION_COUNT as u16).map(Production)
}

/// Audit a Rust dispatcher's registered handlers. A changed action digest invalidates
/// an earlier registration even if the production and its numeric ID are unchanged.
pub fn unmapped_actions(
    has_handler: impl Fn(ActionIdentity) -> bool,
) -> impl Iterator<Item = (Production, ActionIdentity)> {
    productions().filter_map(move |rule| {
        let key = rule.info().action?;
        (!has_handler(key)).then_some((rule, key))
    })
}
