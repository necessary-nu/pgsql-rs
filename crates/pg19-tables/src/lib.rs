//! LALR tables for PostgreSQL 19.0 with the libpg_query compatibility patches.
//! External tokens are the numeric codes from the patched scanner and gram.h.

include!("layout.rs");
const TABLES: &[u8] = include_bytes!("../data/tables.bin");
const INVENTORY: &str = include_str!("../data/productions.tsv");
const SYMBOLS: &str = include_str!("../data/symbols.txt");

pub const INITIAL_STATE: State = State(0);
pub const STATE_COUNT: usize = YYNSTATES as usize;
pub const PRODUCTION_COUNT: usize = RULE_COUNT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State(u16);

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
pub fn terminal(external: u16) -> u16 {
    if usize::from(external) < TOKEN_COUNT {
        u16::from_le_bytes(bytes(YYTRANSLATE, usize::from(external)))
    } else {
        2 // Bison's undefined-token symbol
    }
}

// [spec:pgq:req:grammar.tables]
pub fn action(state: State, lookahead: Option<u16>) -> Action {
    if state.0 == YYFINAL as u16 {
        return Action::Accept;
    }
    let index = usize::from(state.0);
    let base = i32::from_le_bytes(bytes(YYPACT, index));
    if base != YYPACT_NINF {
        let Some(external) = lookahead else {
            return Action::NeedToken;
        };
        if external == 256 {
            return Action::Error; // YYerror means the lexer has already reported an error.
        }
        let token = terminal(external);
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

/// State reached after popping the RHS and pushing the reduction's LHS.
pub fn goto(previous: State, reduced: Production) -> State {
    let nonterminal = usize::from(reduced.lhs()) - YYNTOKENS as usize;
    let index = i32::from_le_bytes(bytes(YYPGOTO, nonterminal)) + i32::from(previous.0);
    match checked_entry(index, previous.0) {
        Some(next) => State(next as u16),
        None => State(u16::from_le_bytes(bytes(YYDEFGOTO, nonterminal))),
    }
}

impl Production {
    pub fn number(self) -> u16 {
        self.0
    }

    pub fn lhs(self) -> u16 {
        u16::from_le_bytes(bytes(YYR1, usize::from(self.0)))
    }

    pub fn rhs_len(self) -> usize {
        usize::from(TABLES[YYR2 + usize::from(self.0)])
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

#[cfg(test)]
mod tests {
    use super::*;

    // Drive reductions without semantic actions so their order can be compared
    // with the grammar's precedence and explicit start modes.
    fn reductions(tokens: &[u16]) -> Result<Vec<&'static str>, usize> {
        let mut stack = vec![INITIAL_STATE];
        let mut reduced = Vec::new();
        let mut position = 0;
        let mut lookahead = None;
        loop {
            let state = *stack.last().unwrap();
            match action(state, lookahead) {
                Action::NeedToken => lookahead = Some(tokens.get(position).copied().unwrap_or(0)),
                Action::Shift(next) => {
                    stack.push(next);
                    position += 1;
                    lookahead = None;
                }
                Action::Reduce(rule) => {
                    stack.truncate(stack.len() - rule.rhs_len());
                    stack.push(goto(*stack.last().unwrap(), rule));
                    reduced.push(rule.info().production);
                }
                Action::Accept => return Ok(reduced),
                Action::Error => return Err(position),
            }
        }
    }

    // [spec:pgq:req:grammar.tables/test]
    #[test]
    fn patched_tokens_and_resolved_conflicts() {
        assert_eq!(symbol_name(terminal(657)), Some("SELECT"));
        assert_eq!(symbol_name(terminal(60)), Some("'<'"));
        assert_eq!(terminal(u16::MAX), terminal(257));
        let trace = reductions(&[657, 266, 43, 266, 42, 266]).unwrap();
        let operators: Vec<_> = trace
            .iter()
            .copied()
            .filter(|r| r.starts_with("a_expr: a_expr"))
            .collect();
        assert_eq!(
            operators,
            ["a_expr: a_expr '*' a_expr", "a_expr: a_expr '+' a_expr"]
        );
        assert_eq!(trace.last(), Some(&"parse_toplevel: stmtmulti"));

        // SET ROLE $1; and REPACK (CONCURRENTLY) orders USING INDEX orders_pkey;
        assert!(reductions(&[664, 641, 267, 59]).is_ok());
        assert!(reductions(&[628, 40, 344, 41, 258, 734, 465, 258, 59]).is_ok());
        assert!(reductions(&[657, 266, 60, 266, 60, 266]).is_err()); // nonassociative '<'
        assert_eq!(reductions(&[657, 41]), Err(1));
        assert!(reductions(&[781, 477])
            .unwrap()
            .ends_with(&["parse_toplevel: MODE_TYPE_NAME Typename"]));
    }

    // [spec:pgq:req:grammar.action-inventory/test]
    #[test]
    fn per_production_action_identity_survives_renumbering() {
        let all = productions().collect::<Vec<_>>();
        assert_eq!(all.len(), 3491);
        assert_eq!(STATE_COUNT, 6590);
        let top = all
            .iter()
            .find(|r| r.info().production == "parse_toplevel: stmtmulti")
            .unwrap();
        let info = top.info();
        assert_eq!(symbol_name(top.lhs()), Some("parse_toplevel"));
        assert_eq!(top.rhs_len(), 1);
        assert!(info.line > 0);
        let expected = info.action.unwrap();
        assert_eq!(
            expected.sha256,
            "10355c52b9c8a6ec77b684b9a9b7680377cb6ac22cca515e47fddf736bb93ca4"
        );

        let explicit = all.iter().filter(|r| r.info().action.is_some()).count();
        assert!(explicit > 1000 && explicit < all.len());
        assert_eq!(unmapped_actions(|_| false).count(), explicit);
        assert_eq!(
            unmapped_actions(|key| key == expected).count(),
            explicit - 1
        );
        assert_eq!(
            unmapped_actions(|key| key.production != expected.production).next(),
            Some((*top, expected))
        );
    }
}
