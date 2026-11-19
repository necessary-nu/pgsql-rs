use super::constraint::*;
use crate::{records::record, value::Value, Failure};
use pg19_engine::Reduction;
use pg19_nodes::*;

pub(crate) const TRIGGER_TYPE_AFTER: i32 = 0;
pub(crate) const TRIGGER_TYPE_BEFORE: i32 = 1 << 1;
pub(crate) const TRIGGER_TYPE_INSERT: i16 = 1 << 2;
pub(crate) const TRIGGER_TYPE_DELETE: i16 = 1 << 3;
pub(crate) const TRIGGER_TYPE_UPDATE: i16 = 1 << 4;
pub(crate) const TRIGGER_TYPE_TRUNCATE: i16 = 1 << 5;
pub(crate) const TRIGGER_TYPE_INSTEAD: i32 = 1 << 6;

fn events(value: Value) -> Result<(i16, Vec<Node>), Failure> {
    match value {
        Value::TriggerEvents(mask, columns) => Ok((mask, columns)),
        _ => Err("expected trigger events".into()),
    }
}

pub(super) fn merge(rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let (left, mut columns) = events(rhs.take(1))?;
    let (right, other) = events(rhs.take(3))?;
    if left & right != 0 {
        return Err(Failure::Syntax("duplicate trigger events specified"));
    }
    columns.extend(other);
    Ok(Value::TriggerEvents(left | right, columns))
}

pub(super) fn create(rhs: &mut Reduction<'_, Value>, constraint: bool) -> Result<Value, Failure> {
    let replace = rhs.take(2).integer()? != 0;
    if constraint {
        let properties = rhs.take(11).integer()?;
        for (bit, label) in [
            (CAS_NOT_VALID, "NOT VALID"),
            (CAS_NO_INHERIT, "NO INHERIT"),
            (CAS_NOT_ENFORCED, "NOT ENFORCED"),
        ] {
            if properties & bit != 0 {
                return Err(Failure::At(
                    format!("constraint triggers cannot be marked {label}"),
                    "0A000",
                    rhs.location(11),
                ));
            }
        }
        if replace {
            return Err(Failure::At(
                "CREATE OR REPLACE CONSTRAINT TRIGGER is not supported".to_owned(),
                "0A000",
                rhs.location(1),
            ));
        }
        let (events, columns) = events(rhs.take(7))?;
        Ok(Value::Node(
            CreateTrigStmt {
                replace,
                isconstraint: true,
                trigname: Some(rhs.take(5).text()?),
                relation: record!(rhs.take(9), RangeVar),
                funcname: rhs.take(18).list()?,
                args: rhs.take(20).list()?,
                row: true,
                timing: TRIGGER_TYPE_AFTER as i16,
                events,
                columns,
                when_clause: rhs.take(15).optional()?,
                transition_rels: vec![],
                deferrable: properties & (CAS_DEFERRABLE | CAS_INITIALLY_DEFERRED) != 0,
                initdeferred: properties & CAS_INITIALLY_DEFERRED != 0,
                constrrel: record!(rhs.take(10), RangeVar),
            }
            .into(),
        ))
    } else {
        let (events, columns) = events(rhs.take(6))?;
        Ok(Value::Node(
            CreateTrigStmt {
                replace,
                trigname: Some(rhs.take(4).text()?),
                relation: record!(rhs.take(8), RangeVar),
                funcname: rhs.take(14).list()?,
                args: rhs.take(16).list()?,
                row: rhs.take(10).integer()? != 0,
                timing: rhs.take(5).integer()? as i16,
                events,
                columns,
                when_clause: rhs.take(11).optional()?,
                transition_rels: rhs.take(9).list()?,
                ..Default::default()
            }
            .into(),
        ))
    }
}
