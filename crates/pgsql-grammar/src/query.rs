//! Statement wrapping and the SELECT productions used by a raw expression.
use super::{
    value::{Quantifier, Value},
    Failure,
};
use crate::raw::*;
use pgsql_engine::Reduction;

pub(super) enum Query {
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

fn statement(value: Value) -> Result<Option<Node>, Failure> {
    match value {
        Value::Empty => Ok(None),
        Value::Node(node) => Ok(Some(node)),
        _ => Err("expected nullable node".into()),
    }
}

pub(super) fn terminate(list: &mut StatementList, position: Location) -> Result<(), Failure> {
    list.terminate_last_at(position)
        .map_err(|_| Failure::Value("invalid statement delimiter location"))
}

// [spec:pgq:sem:grammar.query-actions]
#[inline(always)]
pub(super) fn select<const PRESENT: usize>(
    distinct: bool,
    rhs: &mut Reduction<'_, Value>,
) -> Result<Value, Failure> {
    let mut stmt = SelectStmt {
        target_list: rhs.take(3).list()?,
        ..Default::default()
    };
    if distinct {
        stmt.distinct_clause = rhs.take(2).list()?;
    }
    if PRESENT >= 4 {
        stmt.into_clause = match rhs.take(4) {
            Value::Empty => None,
            Value::Node(Node::IntoClause(n)) => Some(n),
            _ => return Err("expected INTO clause".into()),
        };
    }
    if PRESENT >= 5 {
        stmt.from_clause = rhs.take(5).list()?;
    }
    if PRESENT >= 6 {
        stmt.where_clause = rhs.take(6).optional()?;
    }
    if PRESENT >= 7 {
        (stmt.group_clause, stmt.group_distinct) = rhs.take(7).group()?;
    }
    if PRESENT >= 8 {
        stmt.having_clause = rhs.take(8).optional()?;
    }
    if PRESENT >= 9 {
        stmt.window_clause = rhs.take(9).list()?;
    }
    Ok(Value::from_node(stmt.into()))
}

#[cfg_attr(not(debug_assertions), inline(always))]
pub(super) fn reduce(action: Query, rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    Ok(match action {
        Query::PlStart(nnames) => {
            let mut node = rhs.take(2).node()?;
            if let Some(nnames) = nnames {
                let Node::PLAssignStmt(n) = &mut node else {
                    return Err("expected PLAssignStmt".into());
                };
                n.set_field(|node| &mut node.nnames, nnames);
            }
            Value::from_list(vec![RawStmt::new(node, rhs.location(2)).into()])
        }
        Query::Assignment => {
            let fields = rhs.take(2).list()?;
            super::expr::checked_fields(&fields)?;
            Value::from_node(
                PLAssignStmt {
                    name: Some(rhs.take(1).text()?),
                    indirection: fields,
                    nnames: 0,
                    val: Some(rhs.take(4).select()?),
                    location: rhs.location(1),
                }
                .into(),
            )
        }
        Query::ParameterName => Value::from_text(format!("${}", rhs.take(1).integer()?).into()),
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
            Value::from_node(stmt.into())
        }
        Query::Options {
            with,
            sort,
            locks,
            limit,
        } => {
            let stmt = rhs.take(if with { 2 } else { 1 }).select()?;
            let mut record = stmt.load();
            if let Some(i) = sort {
                let order = rhs.take(i).list()?;
                if !order.is_empty() {
                    if !record.sort_clause.is_empty() {
                        return Err(Failure::At(
                            "multiple ORDER BY clauses not allowed".into(),
                            "42601",
                            read(|arena| {
                                super::location::expression_location(order.slice(arena), arena)
                            }),
                        ));
                    }
                    record.sort_clause = order;
                }
            }
            if let Some(i) = locks {
                record.locking_clause.append(rhs.take(i).list()?);
            }
            if let Some(i) = limit {
                super::limits::apply(&mut record, rhs.take(i), true)?;
            }
            if with {
                let Node::WithClause(with) = rhs.take(1).node()? else {
                    return Err("expected WITH clause".into());
                };
                if record.with_clause.is_some() {
                    return Err(Failure::At(
                        "multiple WITH clauses not allowed".into(),
                        "42601",
                        with.load().location,
                    ));
                }
                record.with_clause = Some(with);
            }
            stmt.set(record);
            Value::from_node(stmt.into())
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
            Value::from_node(
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
            Value::from_node(
                GroupingSet {
                    kind,
                    content,
                    location: rhs.location(1),
                }
                .into(),
            )
        }
        Query::Values(more) => {
            let stmt = if more {
                rhs.take(1).select()?
            } else {
                SelectStmt::default().into()
            };
            let row = rhs.take(if more { 4 } else { 3 }).list()?;
            let mut rows = stmt.load().values_lists;
            rows.push(Node::from(row));
            stmt.set_field(|select| &mut select.values_lists, rows);
            Value::from_node(stmt.into())
        }
        Query::CursorOption(mask) => Value::Integer(rhs.take(1).integer()? | mask),
        Query::Cursor => Value::from_node(
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
            Value::from_node(
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
            let Node::RangeVar(relation) = rhs.take(4).node()? else {
                return Err("expected temporary relation".into());
            };
            relation.set_field(|node| &mut node.relpersistence, b't');
            Value::from_node(relation.into())
        }
        Query::TypeStart => Value::from_list(vec![rhs.take(2).node()?]),
        Query::First => Value::Statements(StatementList::from_first(
            statement(rhs.take(1))?,
            rhs.location(1),
        )),
        Query::Next => {
            let mut list = rhs.take(1).statements()?;
            terminate(&mut list, rhs.location(2))?;
            list.push(statement(rhs.take(3))?, rhs.location(3));
            Value::Statements(list)
        }
        Query::Target(alias) => {
            let name = alias.map(|i| rhs.take(i).text()).transpose()?;
            Value::from_node(
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
            Value::from_node(
                ResTarget {
                    val: Some(Box::new(col.into())),
                    location,
                    ..Default::default()
                }
                .into(),
            )
        }
        Query::Select(distinct) => return select::<9>(distinct, rhs),
        Query::Set(op) => Value::from_node(
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
        Query::Distinct => Value::from_list(vec![Node::Null]),
        Query::Group(false) => Value::Group(Vec::new(), false),
        Query::Group(true) => Value::Group(
            rhs.take(4).list()?,
            rhs.take(3).quantifier()? == Quantifier::Distinct,
        ),
    })
}
