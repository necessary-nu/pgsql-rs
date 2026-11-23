use crate::deparse::{statements, Error};
use crate::raw_walk::{summary_walks_children, RawMut, RawRef};
use pg19_nodes::*;
use std::cmp::Reverse;

#[derive(Clone, Copy)]
enum Attr {
    Targets,
    Where,
    Values,
    Cols,
    Cte,
}

enum Path {
    Root(usize),
    Child { parent: usize, edge: usize },
}

struct Candidate {
    path: usize,
    attr: Attr,
    depth: usize,
    length: usize,
}

fn relation() -> Option<Box<RangeVar>> {
    Some(Box::new(RangeVar {
        relname: Some("x".into()),
        inh: true,
        relpersistence: b'p',
        ..Default::default()
    }))
}

fn wrapped(node: Node) -> RawStmt {
    RawStmt {
        stmt: Some(Box::new(node)),
        ..Default::default()
    }
}

fn candidates(tree: &[RawStmt], style: Style) -> Result<(Vec<Path>, Vec<Candidate>), Error> {
    let mut paths = Vec::new();
    let mut pending = Vec::new();
    for (i, stmt) in tree.iter().enumerate().rev() {
        let Some(n) = stmt.stmt.as_deref() else {
            return Err(Error::InvalidField("RawStmt.stmt"));
        };
        pending.push((RawRef::from(n), paths.len(), 1));
        paths.push(Path::Root(i));
    }

    let mut found = Vec::new();
    while let Some((node, path, depth)) = pending.pop() {
        let mut add = |attr, wrapper, overhead| -> Result<(), Error> {
            let length = statements(&[wrapped(wrapper)])?
                .len()
                .saturating_sub(overhead);
            if length > 3 || matches!(style, Style::PgQueryRs) {
                found.push(Candidate {
                    path,
                    attr,
                    depth,
                    length,
                });
            }
            Ok(())
        };
        let mut where_clause = None;
        let mut assignments = None;
        match node {
            RawRef::SelectStmt(s) => {
                if !s.target_list.is_empty() {
                    let query = SelectStmt {
                        target_list: s.target_list.clone(),
                        ..Default::default()
                    };
                    add(Attr::Targets, query.into(), 7)?;
                }
                if let Some(w) = &s.where_clause {
                    let query = SelectStmt {
                        where_clause: Some(w.clone()),
                        ..Default::default()
                    };
                    add(Attr::Where, query.into(), 13)?;
                }
                if !s.values_lists.is_empty() {
                    let query = SelectStmt {
                        values_lists: s.values_lists.clone(),
                        ..Default::default()
                    };
                    let overhead = match style {
                        Style::Summary => 9,
                        Style::PgQueryRs => 7,
                    };
                    add(Attr::Values, query.into(), overhead)?;
                }
            }
            RawRef::InsertStmt(s) if !s.cols.is_empty() => {
                // The C scoring wrapper sets override to 1 even though it subtracts
                // the length of INSERT INTO x () DEFAULT VALUES.
                let insert = InsertStmt {
                    relation: relation(),
                    cols: s.cols.clone(),
                    r#override: OverridingKind::OVERRIDING_USER_VALUE,
                    ..Default::default()
                };
                add(Attr::Cols, insert.into(), 31)?;
            }
            RawRef::UpdateStmt(s) => {
                assignments = Some(&s.target_list);
                where_clause = s.where_clause.as_ref();
            }
            RawRef::OnConflictClause(s) => {
                assignments = Some(&s.target_list);
                where_clause = s.where_clause.as_ref();
            }
            RawRef::DeleteStmt(s) => where_clause = s.where_clause.as_ref(),
            RawRef::CopyStmt(s) => where_clause = s.where_clause.as_ref(),
            RawRef::RuleStmt(s) => where_clause = s.where_clause.as_ref(),
            RawRef::InferClause(s) => where_clause = s.where_clause.as_ref(),
            RawRef::IndexStmt(s) => where_clause = s.where_clause.as_ref(),
            RawRef::CommonTableExpr(s) => {
                if let Some(q) = s.ctequery.as_deref() {
                    add(Attr::Cte, q.clone(), 0)?;
                }
            }
            _ => {}
        }
        if let Some(targets) = assignments.filter(|t| !t.is_empty()) {
            let update = UpdateStmt {
                relation: relation(),
                target_list: targets.clone(),
                ..Default::default()
            };
            add(Attr::Targets, update.into(), 13)?;
        }
        if let Some(w) = where_clause {
            let query = SelectStmt {
                where_clause: Some(w.clone()),
                ..Default::default()
            };
            add(Attr::Where, query.into(), 13)?;
        }
        if matches!(node, RawRef::List(_)) || node.tag().is_some_and(summary_walks_children) {
            let start = pending.len();
            let mut edge = 0;
            node.children(&mut |child| {
                // The raw walker visits CASE conditions/results directly; our
                // borrowed visitor exposes their list and CaseWhen records.
                let extra = usize::from(
                    !matches!(node, RawRef::CaseWhen(_)) && !matches!(child, RawRef::CaseWhen(_)),
                );
                pending.push((child, paths.len(), depth + extra));
                paths.push(Path::Child { parent: path, edge });
                edge += 1;
            });
            pending[start..].reverse();
        }
    }
    found.sort_by_key(|c| Reverse((c.depth, c.length)));
    Ok((paths, found))
}

fn marker() -> Node {
    ColumnRef {
        fields: vec![Node::String("…".into())],
        ..Default::default()
    }
    .into()
}
fn target() -> Node {
    ResTarget {
        name: Some("…".into()),
        val: Some(Box::new(marker())),
        ..Default::default()
    }
    .into()
}

fn replace(node: RawMut<'_>, attr: Attr) -> Result<(), Error> {
    match (node, attr) {
        (RawMut::SelectStmt(s), Attr::Targets) => s.target_list = vec![target()],
        (RawMut::SelectStmt(s), Attr::Where) => s.where_clause = Some(Box::new(marker())),
        (RawMut::SelectStmt(s), Attr::Values) => s.values_lists = vec![Node::from(vec![marker()])],
        (RawMut::InsertStmt(s), Attr::Cols) => s.cols = vec![target()],
        (RawMut::UpdateStmt(s), Attr::Targets) => s.target_list = vec![target()],
        (RawMut::OnConflictClause(s), Attr::Targets) => s.target_list = vec![target()],
        (RawMut::UpdateStmt(s), Attr::Where) => s.where_clause = Some(Box::new(marker())),
        (RawMut::OnConflictClause(s), Attr::Where) => s.where_clause = Some(Box::new(marker())),
        (RawMut::DeleteStmt(s), Attr::Where) => s.where_clause = Some(Box::new(marker())),
        (RawMut::CopyStmt(s), Attr::Where) => s.where_clause = Some(Box::new(marker())),
        (RawMut::IndexStmt(s), Attr::Where) => s.where_clause = Some(Box::new(marker())),
        (RawMut::RuleStmt(s), Attr::Where) => s.where_clause = Some(Box::new(marker())),
        (RawMut::InferClause(s), Attr::Where) => s.where_clause = Some(Box::new(marker())),
        (RawMut::CommonTableExpr(s), Attr::Cte) => {
            s.ctequery = Some(Box::new(
                SelectStmt {
                    where_clause: Some(Box::new(marker())),
                    ..Default::default()
                }
                .into(),
            ))
        }
        _ => return Err(Error::InvalidField("truncation candidate")),
    }
    Ok(())
}

fn at_path<'a>(
    tree: &'a mut [RawStmt],
    paths: &[Path],
    mut id: usize,
    edges: &mut Vec<usize>,
) -> Option<RawMut<'a>> {
    edges.clear();
    loop {
        match paths[id] {
            Path::Root(i) => {
                let mut item = RawMut::from(tree[i].stmt.as_deref_mut()?);
                for edge in edges.iter().rev() {
                    item = item.child_at(*edge)?;
                }
                return Some(item);
            }
            Path::Child { parent, edge } => {
                edges.push(edge);
                id = parent;
            }
        }
    }
}

// [spec:pgq:req:summary.truncation]
pub(super) fn truncate(nodes: Vec<Node>, limit: i32) -> Result<String, Error> {
    let tree: Vec<_> = nodes
        .into_iter()
        .map(|n| match n {
            Node::RawStmt(s) => Ok(s.into_inner()),
            _ => Err(Error::InvalidField("RawStmt")),
        })
        .collect::<Result<_, _>>()?;
    truncate_summary_statements(tree, limit)
}

/// Apply the summary API’s truncation rules to an already parsed SQL tree.
pub fn truncate_summary_statements(tree: Vec<RawStmt>, limit: i32) -> Result<String, Error> {
    shorten(tree, limit, Style::Summary)
}

#[derive(Clone, Copy)]
enum Style {
    Summary,
    PgQueryRs,
}

/// Shorten an owned tree using pg_query.rs clause selection. It allows eliding
/// short clauses as well, e.g. an INSERT with a single VALUES parameter.
pub fn truncate_statements(tree: Vec<RawStmt>, limit: usize) -> Result<String, Error> {
    shorten(
        tree,
        i32::try_from(limit).unwrap_or(i32::MAX),
        Style::PgQueryRs,
    )
}

fn shorten(mut tree: Vec<RawStmt>, limit: i32, style: Style) -> Result<String, Error> {
    let mut output = statements(&tree)?;
    if output.len() as i64 <= i64::from(limit) {
        return Ok(output);
    }
    let (paths, candidates) = candidates(&tree, style)?;
    let mut edges = Vec::new();
    for candidate in candidates {
        if let Some(node) = at_path(&mut tree, &paths, candidate.path, &mut edges) {
            replace(node, candidate.attr)?;
        }
        output = statements(&tree)?
            .replace("SELECT \"…\" AS \"…\"", "SELECT \"…\"")
            .replace("SELECT WHERE \"…\"", "\"…\"")
            .replace("\"…\"", "...");
        if output.len() <= limit as usize {
            return Ok(output);
        }
    }
    if output.chars().count() > limit as usize {
        let cutoff = limit.saturating_sub(3).max(0) as usize;
        let end = output
            .char_indices()
            .nth(cutoff)
            .map_or(output.len(), |(i, _)| i);
        output.truncate(end);
        output.push_str("...");
    }
    Ok(output)
}
