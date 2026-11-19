//! Range and alias productions in the SELECT grammar.
use super::{value::Value, Failure};
use pg19_engine::Reduction;
use pg19_nodes::*;

pub(super) enum From {
    Alias { name: isize, columns: Option<isize> },
    Table,
    Subquery { lateral: bool },
}

fn alias(value: Value) -> Result<Option<Box<Alias>>, Failure> {
    match value {
        Value::Empty => Ok(None),
        Value::Node(Node::Alias(a)) => Ok(Some(a.into_box())),
        _ => Err("expected Alias".into()),
    }
}

pub(super) fn reduce(op: From, rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    Ok(Value::Node(match op {
        From::Alias { name, columns } => Alias {
            aliasname: Some(rhs.take(name).text()?),
            colnames: columns
                .map(|n| rhs.take(n).list())
                .transpose()?
                .unwrap_or_default(),
        }
        .into(),
        From::Table => {
            let Node::RangeVar(mut table) = rhs.take(1).node()? else {
                return Err("expected RangeVar".into());
            };
            table.alias = alias(rhs.take(2))?;
            table.into()
        }
        From::Subquery { lateral } => {
            let query = if lateral { 2 } else { 1 };
            RangeSubselect {
                lateral,
                subquery: Some(Box::new(rhs.take(query).node()?)),
                alias: alias(rhs.take(query + 1))?,
            }
            .into()
        }
    }))
}
