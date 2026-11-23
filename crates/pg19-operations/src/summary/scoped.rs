use super::{entities::State, tree::TreeRef, Context, Summary};
use pg19_nodes::*;

use std::collections::HashMap;

#[derive(Default)]
struct Bindings<'a>(HashMap<&'a str, usize>);

impl<'a> Bindings<'a> {
    fn bind(&mut self, name: &'a str) {
        *self.0.entry(name).or_default() += 1;
    }

    fn leave(&mut self, clause: &'a WithClause) {
        for name in clause.ctes.iter().filter_map(cte_name) {
            if let std::collections::hash_map::Entry::Occupied(mut entry) = self.0.entry(name) {
                *entry.get_mut() -= 1;
                if *entry.get() == 0 {
                    entry.remove();
                }
            }
        }
    }

    fn physical(&self, r: &RangeVar) -> bool {
        r.schemaname.is_some()
            || r.catalogname.is_some()
            || !r
                .relname
                .as_deref()
                .is_some_and(|name| self.0.contains_key(name))
    }
}

fn cte_name(node: &Node) -> Option<&str> {
    match node {
        Node::CommonTableExpr(cte) => cte.ctename.as_deref(),
        _ => None,
    }
}

#[derive(Clone, Copy, Default)]
struct Position {
    from: bool,
    filter: bool,
}

impl Position {
    fn child(self, parent: TreeRef<'_>, field: &str, statement: bool) -> Self {
        use TreeRef::*;
        let from = match (parent, field) {
            (SelectStmt(_) | UpdateStmt(_), "from_clause")
            | (DeleteStmt(_), "using_clause")
            | (MergeStmt(_), "source_relation") => true,
            (List(_), _) | (JoinExpr(_), "larg" | "rarg") | (RangeTableSample(_), "relation") => {
                self.from
            }
            _ => false,
        };
        Self {
            from,
            filter: field == "where_clause" || (!statement && self.filter),
        }
    }
}

fn clause(node: TreeRef<'_>) -> Option<&WithClause> {
    match node {
        TreeRef::SelectStmt(n) => n.with_clause.as_deref(),
        TreeRef::InsertStmt(n) => n.with_clause.as_deref(),
        TreeRef::DeleteStmt(n) => n.with_clause.as_deref(),
        TreeRef::UpdateStmt(n) => n.with_clause.as_deref(),
        TreeRef::MergeStmt(n) => n.with_clause.as_deref(),
        _ => None,
    }
}

enum Job<'a> {
    Visit(TreeRef<'a>, Position),
    Bind(&'a str),
    Leave(&'a WithClause),
}

fn schedule_ctes<'a>(clause: &'a WithClause, bindings: &mut Bindings<'a>, jobs: &mut Vec<Job<'a>>) {
    // Recursive siblings are all visible before the first body. Otherwise each
    // binding takes effect immediately after its own body has been visited.
    for node in clause.ctes.iter().rev() {
        if let Some(name) = cte_name(node) {
            if clause.recursive {
                bindings.bind(name);
            } else {
                jobs.push(Job::Bind(name));
            }
        }
        jobs.push(Job::Visit(node.into(), Position::default()));
    }
}

fn collect<'a>(state: &mut State<'a>, node: TreeRef<'a>, pos: Position, bindings: &Bindings<'a>) {
    use TreeRef::*;
    match node {
        RangeVar(r) if pos.from && bindings.physical(r) => state.range(Some(r), Context::Select),
        CommonTableExpr(cte) => state.summary.cte_names.extend(cte.ctename.iter().cloned()),
        IntoClause(n) => state.range(n.rel.as_deref(), Context::Ddl),
        InsertStmt(n) => state.range(n.relation.as_deref(), Context::Dml),
        UpdateStmt(n) => state.range(n.relation.as_deref(), Context::Dml),
        DeleteStmt(n) => state.range(n.relation.as_deref(), Context::Dml),
        MergeStmt(n) => state.range(n.relation.as_deref(), Context::Dml),
        CopyStmt(n) => state.range(
            n.relation.as_deref(),
            if n.is_from {
                Context::Dml
            } else {
                Context::Select
            },
        ),
        RepackStmt(n) => state.range(
            n.relation.as_deref().and_then(|r| r.relation.as_deref()),
            Context::Ddl,
        ),
        FuncCall(n) => state.function(&n.funcname, Context::Call),
        ColumnRef(n) if pos.filter => state.column(n),
        CreateTableAsStmt(_) => {} // Its IntoClause owns the destination.
        _ => state.utility(node.into()),
    }
}

/// Collect entities across the complete raw tree with SQL query-block CTE visibility.
pub fn analyze_statements(statements: &[RawStmt]) -> Summary {
    let mut state = State::default();
    let mut bindings = Bindings::default();
    let mut jobs: Vec<_> = statements
        .iter()
        .rev()
        .map(|s| Job::Visit(TreeRef::RawStmt(s), Position::default()))
        .collect();
    while let Some(job) = jobs.pop() {
        let (node, pos) = match job {
            Job::Visit(node, pos) => (node, pos),
            Job::Bind(name) => {
                bindings.bind(name);
                continue;
            }
            Job::Leave(clause) => {
                bindings.leave(clause);
                continue;
            }
        };
        collect(&mut state, node, pos, &bindings);
        let statement = node.statement_name();
        if let Some(name) = statement {
            if !state.summary.statement_types.iter().any(|s| s == name) {
                state.summary.statement_types.push(name.into());
            }
        }
        let with = clause(node);
        if let Some(with) = with {
            jobs.push(Job::Leave(with));
        }
        let start = jobs.len();
        node.children(|field, child| {
            if field != "with_clause" || with.is_none() {
                jobs.push(Job::Visit(
                    child,
                    pos.child(node, field, statement.is_some()),
                ));
            }
        });
        jobs[start..].reverse();
        if let Some(with) = with {
            schedule_ctes(with, &mut bindings, &mut jobs);
        }
    }
    state.finish()
}
