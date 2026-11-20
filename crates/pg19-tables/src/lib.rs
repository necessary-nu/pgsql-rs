//! LALR tables for PostgreSQL 19.0 with the libpg_query compatibility patches.
//! External tokens are the numeric codes from the patched scanner and gram.h.

include!("layout.rs");
const TABLES: &[u8] = include_bytes!("../data/tables.bin");
const INVENTORY: &str = include_str!("../data/productions.tsv");
const SYMBOLS: &str = include_str!("../data/symbols.txt");

include!("lalr.rs");

pub mod plpgsql;

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

    #[test]
    fn compiled_shifts_preserve_bison_actions_in_shared_slots() {
        let table = compile_shifts(|from, to, _| (to, from.number() % 2 == 0));
        let mut retained = 0;
        let mut folded = 0;
        for number in 0..STATE_COUNT {
            let state = State::from_number(number as u16).unwrap();
            for look in terminals()
                .map(Some)
                .chain([None, Some(Lookahead::new(256))])
            {
                match (
                    step(state, look),
                    step_with_shifts(state, look, Some(&table)),
                ) {
                    (Action::Shift(expected), Action::Shift((actual, tag))) => {
                        assert_eq!(expected, actual);
                        if tag {
                            assert_eq!(number % 2, 0);
                            folded += 1;
                        } else if number % 2 == 0 {
                            retained += 1;
                        }
                    }
                    (Action::Reduce(a), Action::Reduce(b)) => assert_eq!(a, b),
                    (Action::NeedToken, Action::NeedToken)
                    | (Action::Accept, Action::Accept)
                    | (Action::Error, Action::Error) => {}
                    pair => panic!("action changed in state {state:?} with {look:?}: {pair:?}"),
                }
            }
        }
        assert!(folded > 0 && retained > 0);
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

        assert!(all
            .iter()
            .all(|r| r.has_action() == r.info().action.is_some()));
        assert!(plpgsql::productions().all(|r| r.has_action() == r.info().action.is_some()));
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
