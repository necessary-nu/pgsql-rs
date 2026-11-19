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
    JsonColumn,
}

pub(super) enum Step<'a> {
    Text(Cow<'a, str>),
    Mark(Location),
    Clause(&'static str),
    Comma,
    Cut,
    Separate,
    Enter,
    Leave,
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
    pub fn cut(&mut self) {
        self.0.push(Step::Cut);
    }
    pub fn separate(&mut self) {
        self.0.push(Step::Separate);
    }
    pub fn enter(&mut self) {
        self.0.push(Step::Enter);
    }
    pub fn leave(&mut self) {
        self.0.push(Step::Leave);
    }
    pub fn comma(&mut self) {
        self.0.push(Step::Comma);
    }
    pub fn select(&mut self, s: &'a SelectStmt, set_arm: bool) {
        self.0.push(Step::Select(s, set_arm));
    }
    pub fn typename(&mut self, t: &'a TypeName) {
        self.0.push(Step::Type(t));
    }
    pub fn statement(&mut self, n: &'a Node) {
        self.0
            .extend([Step::Enter, Step::Statement(n), Step::Leave]);
    }
    pub fn list(&mut self, nodes: &'a [Node], role: Role) {
        for (i, n) in nodes.iter().enumerate() {
            if i > 0 {
                if matches!(role, Role::Target | Role::From | Role::Sort | Role::Group) {
                    self.comma();
                } else {
                    self.text(", ");
                }
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

pub(super) fn statements(
    raw: &[RawStmt],
    options: super::Options,
    comments: &[super::Comment<'_>],
) -> Result<String> {
    if !comments
        .windows(2)
        .all(|pair| pair[0].match_location <= pair[1].match_location)
    {
        return Err(Error::InvalidField("comment locations"));
    }
    let mut comments = comments;
    let mut result = String::new();
    for (idx, s) in raw.iter().enumerate() {
        if idx > 0 {
            result.push_str("; ");
        }
        let end = if idx + 1 == raw.len() {
            comments.len()
        } else {
            let end = s
                .stmt_location
                .byte_offset()
                .unwrap_or(0)
                .saturating_add(usize::try_from(s.stmt_len).unwrap_or(0));
            comments.partition_point(|c| c.match_location < end)
        };
        let (here, next) = comments.split_at(end);
        let rendered = one(s, options, here)?;
        if options.pretty_print && rendered.starts_with('\n') && result.ends_with("; ") {
            result.pop();
        }
        result.push_str(&rendered);
        if idx + 1 < raw.len() && !here.is_empty() && !result.ends_with('\n') {
            // Keep the next separator outside a final single-line comment.
            let last = here.last().expect("nonempty comments");
            if last.text.starts_with("--") && rendered.trim_end().ends_with(last.text) {
                result.push('\n');
            }
        }
        comments = next;
    }
    Ok(result)
}

fn one(
    s: &RawStmt,
    options: super::Options,
    mut comments: &[super::Comment<'_>],
) -> Result<String> {
    let mut out = super::format::Writer::new(options);
    let cutoff = s.stmt_location.byte_offset().map_or(0, |offset| {
        comments.partition_point(|c| c.match_location <= offset)
    });
    let (leading, rest) = comments.split_at(cutoff);
    out.outer_comments(leading, true);
    comments = rest;
    let mut pending = vec![Step::Statement(required(&s.stmt, "RawStmt.stmt")?)];
    let mut plan = Plan::default();
    while let Some(step) = pending.pop() {
        match step {
            Step::Text(t) => out.text(&t),
            Step::Mark(loc) => {
                if let Some(offset) = loc.byte_offset() {
                    while let [c, rest @ ..] = comments {
                        if c.match_location > offset {
                            break;
                        }
                        out.comment(c);
                        comments = rest;
                    }
                }
            }
            Step::Comma => out.comma(),
            Step::Cut => out.cut(),
            Step::Separate => out.separate(),
            Step::Enter => out.enter()?,
            Step::Leave => out.leave(),
            Step::Clause(keyword) => out.clause(keyword),
            Step::Type(t) => expr::typename(t, &mut plan)?,
            Step::Select(s, arm) => query::select(s, arm, &mut plan)?,
            Step::Node(Node::AExpr(a), Role::Restricted) => {
                super::operator::binary(a, &mut plan, true)?
            }
            Step::Node(n, Role::Expr | Role::Restricted) => expr::expression(n, &mut plan)?,
            Step::Node(n, role) => query::item(n, role, &mut plan)?,
            Step::Statement(Node::SelectStmt(s)) => query::select(s, false, &mut plan)?,
            Step::Statement(Node::InsertStmt(s)) => super::dml::insert(s, &mut plan)?,
            Step::Statement(Node::UpdateStmt(s)) => super::dml::update(s, &mut plan)?,
            Step::Statement(Node::DeleteStmt(s)) => super::dml::delete(s, &mut plan)?,
            Step::Statement(Node::MergeStmt(s)) => super::dml::merge(s, &mut plan)?,
            Step::Statement(n) => {
                if !super::ddl::statement(n, &mut plan)? {
                    let mut sql = String::new();
                    super::render(&mut sql, n)?;
                    out.text(&sql);
                }
            }
        }
        pending.extend(plan.0.drain(..).rev());
    }
    out.outer_comments(comments, false);
    out.finish()
}
