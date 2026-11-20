use pg19_tables::{compile_shifts, goto, step, symbol_name, Action};

/// Run unit reductions that cannot request another token. Each of these rules
/// keeps the same location and predecessor on the Bison stack.
pub(super) fn compile(handlers: &[Option<&str>]) -> Vec<u8> {
    compile_shifts(|previous, mut state, token| {
        let mut literal = false;
        let mut steps = 0;
        while let Action::Reduce(rule) = step(state, None) {
            if rule.rhs_len() != 1 {
                break;
            }
            if handlers[usize::from(rule.number())] != Some("Take(1)") {
                if literal {
                    break;
                }
                let kind = symbol_name(token.symbol()).unwrap();
                if !matches!(
                    (handlers[usize::from(rule.number())], kind),
                    (Some("Expr(E::Integer)"), "ICONST")
                        | (Some("Expr(E::Float)"), "FCONST")
                        | (Some("Expr(E::String)"), "SCONST")
                        | (Some("Expr(E::Bits)"), "BCONST" | "XCONST")
                        | (Some("Expr(E::Boolean(true))"), "TRUE_P")
                        | (Some("Expr(E::Boolean(false))"), "FALSE_P")
                        | (Some("Expr(E::Null)"), "NULL_P")
                ) {
                    break;
                }
                literal = true;
            }
            state = goto(previous, rule);
            steps += 1;
            assert!(steps <= handlers.len(), "cycle in shifted unit reductions");
        }
        (state, literal)
    })
}
