use super::{ranges, Arcs};
use pg19_tables::{goto, productions, step, terminals, Action, Production, State};
use std::collections::BTreeMap;
use std::fmt::Write;

// Each value must match the initialization that the shortened parent action uses.
const SELECT_TAIL: &[(&str, &str)] = &[
    ("into_clause", "Empty"),
    ("from_clause", "Nil"),
    ("where_clause", "Empty"),
    ("group_clause", "Query(Q::Group(false))"),
    ("having_clause", "Empty"),
    ("window_clause", "Nil"),
];
const FUNCTION_TAIL: &[(&str, &str)] = &[
    ("within_group_clause", "Nil"),
    ("filter_clause", "Empty"),
    ("null_treatment", "Integer(0)"),
    ("over_clause", "Empty"),
];

fn matching_tokens(start: State, empty: &[Production], parent: Production) -> Vec<u16> {
    let mut state = start;
    let mut steps = Vec::new();
    for &rule in empty {
        steps.push((state, rule));
        state = goto(state, rule);
    }
    steps.push((state, parent));
    terminals()
        .filter(|&token| {
            steps
                .iter()
                .all(|&(s, r)| step(s, Some(token)) == Action::Reduce(r))
        })
        .map(|t| t.symbol())
        .collect()
}

/// Generate a guard only when Bison would reduce each empty clause and its
/// parent with the same lookahead, without a shift or an intervening action.
pub(super) fn guards(handlers: &[Option<&str>], arcs: &Arcs) -> BTreeMap<usize, String> {
    let empty: BTreeMap<_, _> = productions()
        .filter(|r| r.rhs_len() == 0)
        .filter_map(|r| Some((r.info().production.strip_suffix(": %empty")?, r)))
        .collect();
    let mut guards = BTreeMap::<usize, String>::new();
    for parent in productions() {
        let id = usize::from(parent.number());
        let (tail, distinct) = match handlers[id] {
            Some("Query(Q::Select(true))") => (SELECT_TAIL, Some(true)),
            Some("Query(Q::Select(false))") => (SELECT_TAIL, Some(false)),
            Some("Call(crate::call::Call::Aggregate)") => (FUNCTION_TAIL, None),
            _ => continue,
        };
        let rhs = parent.info().production.split_once(": ").unwrap().1;
        let symbols: Vec<_> = rhs.split_whitespace().collect();
        let suffix: Vec<_> = tail.iter().map(|&(name, _)| name).collect();
        if !symbols.ends_with(&suffix) {
            continue;
        }
        let rules: Option<Vec<_>> = tail
            .iter()
            .map(|&(name, action)| {
                empty
                    .get(name)
                    .copied()
                    .filter(|r| handlers[usize::from(r.number())] == Some(action))
            })
            .collect();
        let Some(rules) = rules else { continue };
        // The function action has one undecorated form. SELECT can end after
        // any materialized clause if at least two trailing reductions disappear.
        let count = if distinct.is_some() {
            rules.len() - 1
        } else {
            1
        };
        for first in 0..count {
            let omitted = &rules[first..];
            let prefix = parent.rhs_len() - omitted.len();
            let initial = omitted[0];
            let construct = match distinct {
                Some(flag) => format!("|rhs| query::select::<{prefix}>({flag}, rhs)"),
                None => "call::aggregate::<false>".into(),
            };
            for &previous in arcs[usize::from(initial.lhs())].keys() {
                let state = State::from_number(previous).unwrap();
                let tokens = matching_tokens(state, omitted, parent);
                if tokens.is_empty() {
                    continue;
                }
                let guard = guards.entry(usize::from(initial.number())).or_default();
                let tokens = ranges(&tokens);
                let extra = omitted.len();
                let lhs = parent.lhs();
                writeln!(guard, "if current.number() == {previous} && stack.has_room({extra}) && stack.lookahead().is_some_and(|t| matches!(t.symbol(), {tokens})) {{
                    let previous = stack.reduce_prefix({prefix}, current, span, |p| p, {construct})
                        .map_err(|f| Box::new((Production::from_number({id}).unwrap(), f)))?;
                    return goto_{lhs}(previous, stack, span);
                }}").unwrap();
            }
        }
    }
    guards
}
