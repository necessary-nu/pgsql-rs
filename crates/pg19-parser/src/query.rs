//! Statement wrapping and the SELECT productions used by a raw expression.
use super::{
    value::{Quantifier, Value},
    Failure,
};
use pg19_engine::Reduction;
use pg19_nodes::*;

pub(super) enum Query {
    Statements,
    First,
    Next,
    Target(Option<isize>),
    TargetStar,
    Select(bool),
    Set(SetOperation),
    Quantifier(Quantifier),
    Distinct,
    Group(bool),
}

fn push_statement(
    list: &mut StatementList,
    value: Value,
    location: Location,
) -> Result<(), Failure> {
    list.push(value.optional()?.map(|n| *n), location);
    Ok(())
}

fn select(distinct: bool, rhs: &mut Reduction<'_, Value>) -> Result<SelectStmt, Failure> {
    let targets = rhs.take(3).list()?;
    let (group_clause, group_distinct) = rhs.take(7).group()?;
    let into_clause = match rhs.take(4) {
        Value::Empty => None,
        Value::Node(Node::IntoClause(n)) => Some(n),
        _ => return Err("expected INTO clause".into()),
    };
    Ok(SelectStmt {
        distinct_clause: if distinct {
            rhs.take(2).list()?
        } else {
            Vec::new()
        },
        target_list: targets,
        into_clause,
        from_clause: rhs.take(5).list()?,
        where_clause: rhs.take(6).optional()?,
        group_clause,
        group_distinct,
        having_clause: rhs.take(8).optional()?,
        window_clause: rhs.take(9).list()?,
        ..Default::default()
    })
}

pub(super) fn reduce(action: Query, rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    Ok(match action {
        Query::Statements => {
            let list = rhs.take(1).statements()?;
            Value::List(list.into_statements().into_iter().map(Node::from).collect())
        }
        Query::First => {
            let mut list = StatementList::default();
            push_statement(&mut list, rhs.take(1), rhs.location(1))?;
            Value::Statements(list)
        }
        Query::Next => {
            let mut list = rhs.take(1).statements()?;
            list.terminate_last_at(rhs.location(2))
                .map_err(|_| "invalid statement delimiter location")?;
            push_statement(&mut list, rhs.take(3), rhs.location(3))?;
            Value::Statements(list)
        }
        Query::Target(alias) => {
            let name = alias.map(|i| rhs.take(i).text()).transpose()?;
            Value::Node(
                ResTarget {
                    name,
                    indirection: Vec::new(),
                    val: Some(Box::new(rhs.take(1).node()?)),
                    location: rhs.location(1),
                }
                .into(),
            )
        }
        Query::TargetStar => {
            let location = rhs.location(1);
            let col = ColumnRef {
                fields: vec![AStar {}.into()],
                location,
            };
            Value::Node(
                ResTarget {
                    val: Some(Box::new(col.into())),
                    location,
                    ..Default::default()
                }
                .into(),
            )
        }
        Query::Select(distinct) => Value::Node(select(distinct, rhs)?.into()),
        Query::Set(op) => Value::Node(
            SelectStmt {
                op,
                all: rhs.take(3).quantifier()? == Quantifier::All,
                larg: Some(rhs.take(1).select()?),
                rarg: Some(rhs.take(4).select()?),
                ..Default::default()
            }
            .into(),
        ),
        Query::Quantifier(q) => Value::Quantifier(q),
        Query::Distinct => Value::List(vec![Node::Null]),
        Query::Group(false) => Value::Group(Vec::new(), false),
        Query::Group(true) => Value::Group(
            rhs.take(4).list()?,
            rhs.take(3).quantifier()? == Quantifier::Distinct,
        ),
    })
}
