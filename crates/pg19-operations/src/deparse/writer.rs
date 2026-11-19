//! Expand borrowed tree edges on a worklist. Neither expression nesting nor
//! left-deep set operations consume a Rust call frame for each raw node.
use super::{expr, query, Error, Result};
use pg19_nodes::*;
use std::borrow::Cow;

#[derive(Clone, Copy)]
pub(super) enum Role {
    Expr,
    Restricted,
    Target,
    From,
    Sort,
    Group,
    Cte,
}

pub(super) enum Step<'a> {
    Text(Cow<'a, str>),
    Mark(Location),
    Clause(&'static str),
    Comma,
    Node(&'a Node, Role),
    Select(&'a SelectStmt, bool),
    Type(&'a TypeName),
    Statement(&'a Node),
}

#[derive(Default)]
pub(super) struct Plan<'a>(Vec<Step<'a>>);

impl<'a> Plan<'a> {
    pub fn text(&mut self, s: impl Into<Cow<'a, str>>) {
        self.0.push(Step::Text(s.into()));
    }
    pub fn mark(&mut self, loc: Location) {
        self.0.push(Step::Mark(loc));
    }
    pub fn clause(&mut self, word: &'static str) {
        self.0.push(Step::Clause(word));
    }
    pub fn node(&mut self, n: &'a Node, role: Role) {
        self.0.push(Step::Node(n, role));
    }
    pub fn expr(&mut self, n: &'a Node) {
        self.0.push(Step::Node(n, Role::Expr));
    }
    pub fn select(&mut self, s: &'a SelectStmt, set_arm: bool) {
        self.0.push(Step::Select(s, set_arm));
    }
    pub fn typename(&mut self, t: &'a TypeName) {
        self.0.push(Step::Type(t));
    }
    pub fn statement(&mut self, n: &'a Node) {
        self.0.push(Step::Statement(n));
    }
    pub fn list(&mut self, nodes: &'a [Node], role: Role) {
        for (i, n) in nodes.iter().enumerate() {
            if i > 0 {
                self.0.push(Step::Comma);
            }
            self.node(n, role);
        }
    }
    pub fn wrapped(&mut self, n: &'a Node, role: Role, parens: bool) {
        if parens {
            self.text("(");
        }
        self.node(n, role);
        if parens {
            self.text(")");
        }
    }
}

pub(super) fn required<'a, T>(v: &'a Option<Box<T>>, field: &'static str) -> Result<&'a T> {
    match v {
        Some(child) => Ok(child),
        None => Err(Error::InvalidField(field)),
    }
}
pub(super) fn names(values: &[Node], separator: &str) -> Result<String> {
    values
        .iter()
        .map(|n| match n {
            Node::String(s) => super::ident(s),
            _ => Err(Error::InvalidField("identifier list")),
        })
        .collect::<Result<Vec<_>>>()
        .map(|v| v.join(separator))
}

pub(super) fn statements(raw: &[RawStmt]) -> Result<String> {
    let mut pending = Vec::new();
    for (i, s) in raw.iter().enumerate().rev() {
        pending.push(Step::Statement(required(&s.stmt, "RawStmt.stmt")?));
        pending.push(Step::Mark(s.stmt_location));
        if i > 0 {
            pending.push(Step::Text("; ".into()));
        }
    }
    let mut out = String::new();
    let mut plan = Plan::default();
    while let Some(step) = pending.pop() {
        match step {
            Step::Text(s) => out.push_str(&s),
            Step::Mark(loc) => {
                let _ = loc;
            }
            Step::Comma => out.push_str(", "),
            Step::Clause(keyword) => {
                if !out.is_empty() && !out.ends_with([' ', '(']) {
                    out.push(' ');
                }
                out.push_str(keyword);
            }
            Step::Type(t) => expr::typename(t, &mut plan)?,
            Step::Select(s, arm) => query::select(s, arm, &mut plan)?,
            Step::Node(Node::AExpr(a), Role::Restricted) => {
                super::operator::binary(a, &mut plan, true)?
            }
            Step::Node(n, Role::Expr | Role::Restricted) => expr::expression(n, &mut plan)?,
            Step::Node(n, role) => query::item(n, role, &mut plan)?,
            Step::Statement(Node::SelectStmt(s)) => query::select(s, false, &mut plan)?,
            Step::Statement(n) => super::render(&mut out, n)?,
        }
        pending.extend(plan.0.drain(..).rev());
    }
    Ok(out)
}
