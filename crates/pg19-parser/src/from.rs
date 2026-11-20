//! Range and alias productions in the SELECT grammar.
use super::{
    value::{OwnedValue, Value},
    Failure,
};
use pg19_engine::Reduction;
use pg19_nodes::*;

pub(super) enum From {
    Alias { name: isize, columns: Option<isize> },
    Table,
    Join(bool),
    JoinUsing,
    JoinAlias,
    Function(bool),
    FunctionAlias(isize, isize),
    FunctionColumn,
    Sample,
    XmlTable(bool),
    JsonTable(bool),
    Subquery { lateral: bool },
}

fn alias(value: Value) -> Result<Option<Box<Alias>>, Failure> {
    match value {
        Value::Empty => Ok(None),
        Value::Owned(OwnedValue::Node(Node::Alias(a))) => Ok(Some(a.into_box())),
        _ => Err("expected Alias".into()),
    }
}

fn optional_alias(node: Node) -> Result<Option<Box<Alias>>, Failure> {
    match node {
        Node::Null => Ok(None),
        Node::Alias(a) => Ok(Some(a.into_box())),
        _ => Err("expected alias".into()),
    }
}

fn pair(value: Value) -> Result<[Node; 2], Failure> {
    value
        .list()?
        .try_into()
        .map_err(|_| "expected two list cells".into())
}

fn columns(node: Node) -> Result<Vec<Node>, Failure> {
    match node {
        Node::Null => Ok(Vec::new()),
        Node::List(v) => Ok(v.into_vec()),
        _ => Err("expected column list".into()),
    }
}

#[cfg_attr(not(debug_assertions), inline(always))]
pub(super) fn reduce(op: From, rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    Ok(Value::from_node(match op {
        From::Alias { name, columns } => Alias {
            aliasname: Some(rhs.take(name).text()?),
            colnames: columns
                .map(|n| rhs.take(n).list())
                .transpose()?
                .unwrap_or_default(),
        }
        .into(),
        From::Join(typed) => {
            let right = if typed { 4 } else { 3 };
            let mut join = JoinExpr {
                jointype: if typed {
                    JoinType(rhs.take(2).integer()?)
                } else {
                    JoinType::JOIN_INNER
                },
                larg: Some(Box::new(rhs.take(1).node()?)),
                rarg: Some(Box::new(rhs.take(right).node()?)),
                ..Default::default()
            };
            match rhs.take(right + 1) {
                Value::Owned(OwnedValue::Node(node)) => join.quals = Some(Box::new(node)),
                v => {
                    let [names, alias] = pair(v)?;
                    join.using_clause = columns(names)?;
                    join.join_using_alias = optional_alias(alias)?;
                }
            }
            join.into()
        }
        From::JoinUsing => {
            let names = Node::from(rhs.take(3).list()?);
            let alias = rhs.take(5).optional()?.map_or(Node::Null, |n| *n);
            return Ok(Value::from_list(vec![names, alias]));
        }
        From::JoinAlias => {
            let Node::JoinExpr(mut join) = rhs.take(2).node()? else {
                return Err("expected JOIN".into());
            };
            join.alias = alias(rhs.take(4))?;
            join.into()
        }
        From::Function(lateral) => {
            let i = if lateral { 2 } else { 1 };
            let Node::RangeFunction(mut function) = rhs.take(i).node()? else {
                return Err("expected range function".into());
            };
            let [name, definition] = pair(rhs.take(i + 1))?;
            function.alias = optional_alias(name)?;
            function.coldeflist = columns(definition)?;
            function.lateral = lateral;
            function.into()
        }
        From::FunctionColumn => {
            let Node::TypeName(typ) = rhs.take(2).node()? else {
                return Err("expected column type".into());
            };
            let coll_clause = match rhs.take(3) {
                Value::Empty => None,
                Value::Owned(OwnedValue::Node(Node::CollateClause(v))) => Some(v.into_box()),
                _ => return Err("expected COLLATE".into()),
            };
            ColumnDef {
                colname: Some(rhs.take(1).text()?),
                type_name: Some(typ.into_box()),
                is_local: true,
                coll_clause,
                location: rhs.location(1),
                ..Default::default()
            }
            .into()
        }
        From::FunctionAlias(name, defs) => {
            let alias = Alias {
                aliasname: Some(rhs.take(name).text()?),
                colnames: Vec::new(),
            };
            return Ok(Value::from_list(vec![
                alias.into(),
                Node::from(rhs.take(defs).list()?),
            ]));
        }
        From::Sample => {
            let Node::RangeVar(mut relation) = rhs.take(1).node()? else {
                return Err("expected RangeVar".into());
            };
            let Node::RangeTableSample(mut sample) = rhs.take(3).node()? else {
                return Err("expected TABLESAMPLE".into());
            };
            relation.alias = alias(rhs.take(2))?;
            sample.relation = Some(Box::new(relation.into()));
            sample.into()
        }
        From::XmlTable(lateral) => {
            let i = if lateral { 2 } else { 1 };
            let Node::RangeTableFunc(mut table) = rhs.take(i).node()? else {
                return Err("expected XMLTABLE".into());
            };
            table.lateral = lateral;
            table.alias = alias(rhs.take(i + 1))?;
            table.into()
        }
        From::JsonTable(lateral) => {
            let i = if lateral { 2 } else { 1 };
            let Node::JsonTable(mut table) = rhs.take(i).node()? else {
                return Err("expected JSON_TABLE".into());
            };
            table.lateral = lateral;
            table.alias = alias(rhs.take(i + 1))?;
            table.into()
        }
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
