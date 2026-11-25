//! INSERT statement assembly and assignment targets from PostgreSQL gram.y.
use super::{value::Value, Failure};
use crate::raw::*;
use pgsql_engine::Reduction;

pub(super) enum Dml {
    Insert,
    MergeWhen,
    RelationAlias(isize),
    Target,
    Assignment,
    MultiAssignment,
}

// [spec:pgq:sem:grammar.dml-actions]
#[cfg_attr(not(debug_assertions), inline(always))]
pub(super) fn reduce(op: Dml, rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    Ok(match op {
        Dml::MergeWhen => {
            let Node::MergeWhenClause(action) = rhs.take(4).node()? else {
                return Err("expected MERGE action".into());
            };
            action.set_field(
                |node| &mut node.match_kind,
                MergeMatchKind(rhs.take(1).integer()?),
            );
            action.set_field(|node| &mut node.condition, rhs.take(2).optional()?);
            Value::from_node(action.into())
        }
        Dml::Insert => {
            let Node::InsertStmt(insert) = rhs.take(5).node()? else {
                return Err("expected InsertStmt".into());
            };
            let mut record = insert.load();
            let Node::RangeVar(relation) = rhs.take(4).node()? else {
                return Err("expected INSERT relation".into());
            };
            record.relation = Some(relation);
            record.on_conflict_clause = match rhs.take(6) {
                Value::Empty => None,
                Value::Node(Node::OnConflictClause(v)) => Some(v),
                _ => return Err("expected ON CONFLICT".into()),
            };
            record.returning_clause = match rhs.take(7) {
                Value::Empty => None,
                Value::Node(Node::ReturningClause(v)) => Some(v),
                _ => return Err("expected RETURNING".into()),
            };
            record.with_clause = match rhs.take(1) {
                Value::Empty => None,
                Value::Node(Node::WithClause(v)) => Some(v),
                _ => return Err("expected WITH".into()),
            };
            insert.set(record);
            Value::from_node(insert.into())
        }
        Dml::RelationAlias(i) => {
            let Node::RangeVar(relation) = rhs.take(1).node()? else {
                return Err("expected relation".into());
            };
            relation.set_field(
                |node| &mut node.alias,
                Some(Box::new(Alias {
                    aliasname: Some(rhs.take(i).text()?),
                    colnames: Vec::new(),
                })),
            );
            Value::from_node(relation.into())
        }
        Dml::Target => {
            let indirection = rhs.take(2).list()?;
            super::expr::checked_fields(&indirection)?;
            Value::from_node(
                ResTarget {
                    name: Some(rhs.take(1).text()?),
                    indirection,
                    val: None,
                    location: rhs.location(1),
                }
                .into(),
            )
        }
        Dml::MultiAssignment => {
            let targets = rhs.take(2).list()?;
            let source = Arc::from(Box::new(rhs.take(5).node()?));
            let ncolumns =
                i32::try_from(targets.len()).map_err(|_| "too many assignment targets")?;
            for (target, colno) in targets.iter().zip(1..=ncolumns) {
                let Node::ResTarget(target) = target else {
                    return Err("expected assignment target".into());
                };
                target.set_field(
                    |node| &mut node.val,
                    Some(Box::new(
                        MultiAssignRef {
                            source: Some(source),
                            colno,
                            ncolumns,
                        }
                        .into(),
                    )),
                );
            }
            Value::from_list(targets)
        }
        Dml::Assignment => {
            let Node::ResTarget(target) = rhs.take(1).node()? else {
                return Err("expected assignment target".into());
            };
            target.set_field(|node| &mut node.val, Some(Box::new(rhs.take(3).node()?)));
            Value::from_list(vec![target.into()])
        }
    })
}
