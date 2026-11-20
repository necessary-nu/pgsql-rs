//! SQL:2008 FETCH, LIMIT and OFFSET from PostgreSQL gram.y.
use super::{
    types::int_constant,
    value::{OwnedValue, Value},
    Failure,
};
use pg19_engine::Reduction;
use pg19_nodes::*;

pub(crate) struct SelectLimit {
    offset: Option<Box<Node>>,
    count: Option<Box<Node>>,
    option: LimitOption,
    offset_location: Location,
    count_location: Location,
    option_location: Location,
}

impl Default for SelectLimit {
    fn default() -> Self {
        Self {
            offset: None,
            count: None,
            option: LimitOption::LIMIT_OPTION_COUNT,
            offset_location: Location::UNKNOWN,
            count_location: Location::UNKNOWN,
            option_location: Location::UNKNOWN,
        }
    }
}

pub(super) enum Limit {
    Count {
        count: Option<isize>,
        ties: Option<isize>,
    },
    Offset {
        offset: isize,
        count: Option<isize>,
    },
    Comma,
}

#[cfg_attr(not(debug_assertions), inline(always))]
pub(super) fn reduce(op: Limit, rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let limit = match op {
        Limit::Count { count, ties } => SelectLimit {
            count: Some(Box::new(match count {
                Some(i) => rhs.take(i).node()?,
                None => int_constant(1, Location::UNKNOWN),
            })),
            option: if ties.is_some() {
                LimitOption::LIMIT_OPTION_WITH_TIES
            } else {
                LimitOption::LIMIT_OPTION_COUNT
            },
            count_location: rhs.location(1),
            option_location: ties.map_or(Location::UNKNOWN, |i| rhs.location(i)),
            ..Default::default()
        },
        Limit::Offset { offset, count } => {
            let mut limit = if let Some(i) = count {
                match rhs.take(i) {
                    Value::Owned(OwnedValue::Limit(v)) => *v,
                    _ => return Err("expected LIMIT".into()),
                }
            } else {
                SelectLimit::default()
            };
            limit.offset = Some(Box::new(rhs.take(offset).node()?));
            limit.offset_location = rhs.location(offset);
            limit
        }
        Limit::Comma => {
            return Err(Failure::At(
                "LIMIT #,# syntax is not supported".into(),
                "42601",
                rhs.location(1),
            ))
        }
    };
    Ok(Value::Owned(OwnedValue::Limit(Box::new(limit))))
}

pub(super) fn apply(stmt: &mut SelectStmt, limit: Value, check_lock: bool) -> Result<(), Failure> {
    let limit = match limit {
        Value::Empty => return Ok(()),
        Value::Owned(OwnedValue::Limit(v)) => v,
        _ => return Err("expected LIMIT".into()),
    };
    if let Some(offset) = limit.offset {
        if stmt.limit_offset.is_some() {
            return Err(Failure::At(
                "multiple OFFSET clauses not allowed".into(),
                "42601",
                limit.offset_location,
            ));
        }
        stmt.limit_offset = Some(offset);
    }
    if let Some(count) = limit.count {
        if stmt.limit_count.is_some() {
            return Err(Failure::At(
                "multiple LIMIT clauses not allowed".into(),
                "42601",
                limit.count_location,
            ));
        }
        stmt.limit_count = Some(count);
    }
    if limit.option == LimitOption::LIMIT_OPTION_WITH_TIES {
        if stmt.sort_clause.is_empty() {
            return Err(Failure::At(
                "WITH TIES cannot be specified without ORDER BY clause".into(),
                "42601",
                limit.option_location,
            ));
        }
        if check_lock && stmt.locking_clause.iter().any(|n|matches!(n,Node::LockingClause(l) if l.wait_policy == LockWaitPolicy::LockWaitSkip)) {
            return Err(Failure::At("SKIP LOCKED and WITH TIES options cannot be used together".into(),"42601",limit.option_location));
        }
    }
    stmt.limit_option = limit.option;
    Ok(())
}
