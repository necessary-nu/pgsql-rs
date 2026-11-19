//! Statement wrapping and the SELECT productions used by a raw expression.
use super::{
    value::{Quantifier, Value},
    Failure,
};
use pg19_engine::Reduction;
use pg19_nodes::*;

pub(super) enum Query {
    Statements,
    TypeStart,
    Cursor,
    CursorOption(i32),
    Cycle,
    GlobalTemp,
    PlStart(Option<i32>),
    PlExpression,
    Assignment,
    ParameterName,
    Values(bool),
    Table,
    GroupingSet(GroupingSetKind, Option<isize>),
    Options {
        with: bool,
        sort: Option<isize>,
        locks: Option<isize>,
        limit: Option<isize>,
    },
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
        Value::Node(Node::IntoClause(n)) => Some(n.into_box()),
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
        Query::PlStart(nnames) => {
            let mut node = rhs.take(2).node()?;
            if let Some(nnames) = nnames {
                let Node::PLAssignStmt(n) = &mut node else {
                    return Err("expected PLAssignStmt".into());
                };
                n.nnames = nnames;
            }
            Value::List(vec![RawStmt::new(node, rhs.location(2)).into()])
        }
        Query::Assignment => {
            let fields = rhs.take(2).list()?;
            super::expr::checked_fields(&fields)?;
            Value::Node(
                PLAssignStmt {
                    name: Some(rhs.take(1).text()?),
                    indirection: fields,
                    nnames: 0,
                    val: Some(rhs.take(4).select()?.into_box()),
                    location: rhs.location(1),
                }
                .into(),
            )
        }
        Query::ParameterName => Value::Text(format!("${}", rhs.take(1).integer()?)),
        Query::PlExpression => {
            let (group_clause, group_distinct) = rhs.take(5).group()?;
            let mut stmt = SelectStmt {
                distinct_clause: rhs.take(1).list()?,
                target_list: rhs.take(2).list()?,
                from_clause: rhs.take(3).list()?,
                where_clause: rhs.take(4).optional()?,
                group_clause,
                group_distinct,
                having_clause: rhs.take(6).optional()?,
                window_clause: rhs.take(7).list()?,
                sort_clause: rhs.take(8).list()?,
                locking_clause: rhs.take(10).list()?,
                ..Default::default()
            };
            super::limits::apply(&mut stmt, rhs.take(9), false)?;
            Value::Node(stmt.into())
        }
        Query::Options {
            with,
            sort,
            locks,
            limit,
        } => {
            let mut stmt = rhs.take(if with { 2 } else { 1 }).select()?;
            if let Some(i) = sort {
                let order = rhs.take(i).list()?;
                if !order.is_empty() {
                    if !stmt.sort_clause.is_empty() {
                        return Err(Failure::At(
                            "multiple ORDER BY clauses not allowed".into(),
                            "42601",
                            super::location::expression_location(&order),
                        ));
                    }
                    stmt.sort_clause = order;
                }
            }
            if let Some(i) = locks {
                stmt.locking_clause.extend(rhs.take(i).list()?);
            }
            if let Some(i) = limit {
                super::limits::apply(&mut stmt, rhs.take(i), true)?;
            }
            if with {
                let Node::WithClause(with) = rhs.take(1).node()? else {
                    return Err("expected WITH clause".into());
                };
                if stmt.with_clause.is_some() {
                    return Err(Failure::At(
                        "multiple WITH clauses not allowed".into(),
                        "42601",
                        with.location,
                    ));
                }
                stmt.with_clause = Some(with.into_box());
            }
            Value::Node(stmt.into())
        }
        Query::Table => {
            let column = ColumnRef {
                fields: vec![AStar {}.into()],
                location: Location::UNKNOWN,
            };
            let target = ResTarget {
                val: Some(Box::new(column.into())),
                location: Location::UNKNOWN,
                ..Default::default()
            };
            Value::Node(
                SelectStmt {
                    target_list: vec![target.into()],
                    from_clause: vec![rhs.take(2).node()?],
                    ..Default::default()
                }
                .into(),
            )
        }
        Query::GroupingSet(kind, items) => {
            let content = items
                .map(|i| rhs.take(i).list())
                .transpose()?
                .unwrap_or_default();
            Value::Node(
                GroupingSet {
                    kind,
                    content,
                    location: rhs.location(1),
                }
                .into(),
            )
        }
        Query::Values(more) => {
            let mut stmt = if more {
                rhs.take(1).select()?
            } else {
                SelectStmt::default().into()
            };
            let row = rhs.take(if more { 4 } else { 3 }).list()?;
            stmt.values_lists.push(Node::from(row));
            Value::Node(stmt.into())
        }
        Query::CursorOption(mask) => Value::Integer(rhs.take(1).integer()? | mask),
        Query::Cursor => Value::Node(
            DeclareCursorStmt {
                portalname: Some(rhs.take(2).text()?),
                options: rhs.take(3).integer()? | rhs.take(5).integer()? | 0x100,
                query: Some(Box::new(rhs.take(7).node()?)),
            }
            .into(),
        ),
        Query::Cycle => {
            let flag = |val| {
                Some(Box::new(Node::AConst(AConst {
                    val: Literal::Boolean(val),
                    location: Location::UNKNOWN,
                })))
            };
            Value::Node(
                CTECycleClause {
                    cycle_col_list: rhs.take(2).list()?,
                    cycle_mark_column: Some(rhs.take(4).text()?),
                    cycle_mark_value: flag(true),
                    cycle_mark_default: flag(false),
                    cycle_path_column: Some(rhs.take(6).text()?),
                    location: rhs.location(1),
                    ..Default::default()
                }
                .into(),
            )
        }
        Query::GlobalTemp => {
            let Node::RangeVar(mut relation) = rhs.take(4).node()? else {
                return Err("expected temporary relation".into());
            };
            relation.relpersistence = b't';
            Value::Node(relation.into())
        }
        Query::TypeStart => Value::List(vec![rhs.take(2).node()?]),
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
