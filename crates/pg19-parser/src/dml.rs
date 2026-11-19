//! INSERT statement assembly and assignment targets from PostgreSQL gram.y.
use super::{value::Value, Failure};
use pg19_engine::Reduction;
use pg19_nodes::*;

pub(super) enum Dml {
    Insert,
    MergeWhen,
    RelationAlias(isize),
    Target,
    Assignment,
    MultiAssignment,
}

// [spec:pgq:sem:grammar.dml-actions]
pub(super) fn reduce(op: Dml, rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    Ok(match op {
        Dml::MergeWhen => {
            let Node::MergeWhenClause(mut action) = rhs.take(4).node()? else {
                return Err("expected MERGE action".into());
            };
            action.match_kind = MergeMatchKind(rhs.take(1).integer()?);
            action.condition = rhs.take(2).optional()?;
            Value::Node(action.into())
        }
        Dml::Insert => {
            let Node::InsertStmt(mut insert) = rhs.take(5).node()? else {
                return Err("expected InsertStmt".into());
            };
            let Node::RangeVar(relation) = rhs.take(4).node()? else {
                return Err("expected INSERT relation".into());
            };
            insert.relation = Some(relation.into_box());
            insert.on_conflict_clause = match rhs.take(6) {
                Value::Empty => None,
                Value::Node(Node::OnConflictClause(v)) => Some(v.into_box()),
                _ => return Err("expected ON CONFLICT".into()),
            };
            insert.returning_clause = match rhs.take(7) {
                Value::Empty => None,
                Value::Node(Node::ReturningClause(v)) => Some(v.into_box()),
                _ => return Err("expected RETURNING".into()),
            };
            insert.with_clause = match rhs.take(1) {
                Value::Empty => None,
                Value::Node(Node::WithClause(v)) => Some(v.into_box()),
                _ => return Err("expected WITH".into()),
            };
            Value::Node(insert.into())
        }
        Dml::RelationAlias(i) => {
            let Node::RangeVar(mut relation) = rhs.take(1).node()? else {
                return Err("expected relation".into());
            };
            relation.alias = Some(Box::new(Alias {
                aliasname: Some(rhs.take(i).text()?),
                colnames: Vec::new(),
            }));
            Value::Node(relation.into())
        }
        Dml::Target => {
            let indirection = rhs.take(2).list()?;
            super::expr::checked_fields(&indirection)?;
            Value::Node(
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
            let mut targets = rhs.take(2).list()?;
            let source = std::sync::Arc::new(rhs.take(5).node()?);
            let ncolumns =
                i32::try_from(targets.len()).map_err(|_| "too many assignment targets")?;
            for (target, colno) in targets.iter_mut().zip(1..=ncolumns) {
                let Node::ResTarget(target) = target else {
                    return Err("expected assignment target".into());
                };
                target.val = Some(Box::new(
                    MultiAssignRef {
                        source: Some(source.clone()),
                        colno,
                        ncolumns,
                    }
                    .into(),
                ));
            }
            Value::List(targets)
        }
        Dml::Assignment => {
            let Node::ResTarget(mut target) = rhs.take(1).node()? else {
                return Err("expected assignment target".into());
            };
            target.val = Some(Box::new(rhs.take(3).node()?));
            Value::List(vec![target.into()])
        }
    })
}
