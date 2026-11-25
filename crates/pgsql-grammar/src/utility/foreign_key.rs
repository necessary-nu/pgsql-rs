use super::constraint;
use crate::raw::*;
use crate::{records::record, value::Value, Failure};
use pgsql_engine::Reduction;

pub(super) fn update(rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let action = rhs.take(3);
    match &action {
        Value::KeyAction(_, columns) if columns.is_empty() => Ok(action),
        Value::KeyAction(kind, _) => {
            let action_name = if *kind == b'n' {
                "SET NULL"
            } else {
                "SET DEFAULT"
            };
            Err(Failure::At(
                format!("a column list with {action_name} is only supported for ON DELETE actions"),
                "0A000",
                rhs.location(1),
            ))
        }
        _ => Err("expected foreign key action".into()),
    }
}

fn action_at(
    rhs: &mut Reduction<'_, Value>,
    index: Option<isize>,
) -> Result<(u8, Vec<Node>), Failure> {
    let Some(index) = index else {
        return Ok((b'a', Vec::new()));
    };
    match rhs.take(index) {
        Value::KeyAction(kind, columns) => Ok((kind, columns)),
        _ => Err("expected foreign key action".into()),
    }
}

pub(super) fn actions(
    rhs: &mut Reduction<'_, Value>,
    update: Option<isize>,
    delete: Option<isize>,
) -> Result<Value, Failure> {
    let (update, _) = action_at(rhs, update)?;
    let (delete, columns) = action_at(rhs, delete)?;
    Ok(Value::KeyActions(update, delete, columns))
}

pub(super) fn foreign_key(rhs: &mut Reduction<'_, Value>, table: bool) -> Result<Value, Failure> {
    let rel = if table { 8 } else { 2 };
    let Value::KeyActions(update, delete, set_columns) = rhs.take(rel + 3) else {
        return Err("expected foreign key actions".into());
    };
    let mut node = Constraint {
        contype: ConstrType::CONSTR_FOREIGN,
        location: rhs.location(1),
        pktable: record!(rhs.take(rel), RangeVar),
        fk_matchtype: rhs.take(rel + 2).integer()? as u8,
        fk_upd_action: update,
        fk_del_action: delete,
        fk_del_set_cols: set_columns,
        is_enforced: true,
        initially_valid: true,
        ..Default::default()
    };
    if table {
        node.fk_attrs = rhs.take(4).list()?;
        if let Some(period) = rhs.take(5).optional()? {
            node.fk_attrs.push(period.load());
            node.fk_with_period = true;
        }
        let mut columns = rhs.take(9).list()?.into_iter();
        node.pk_attrs = match columns.next() {
            Some(Node::Null) => Vec::new(),
            Some(Node::List(list)) => list,
            _ => return Err("expected referenced columns".into()),
        };
        if let Some(period) = columns.next().filter(|node| !matches!(node, Node::Null)) {
            node.pk_attrs.push(period);
            node.pk_with_period = true;
        }
        constraint::process(
            &mut node,
            rhs.take(12).integer()?,
            rhs.location(12),
            "FOREIGN KEY",
            constraint::CAS_DEFERRABLE
                | constraint::CAS_INITIALLY_DEFERRED
                | constraint::CAS_ENFORCED
                | constraint::CAS_NOT_VALID,
        )?;
        node.initially_valid = !node.skip_validation;
    } else {
        node.pk_attrs = rhs.take(3).list()?;
    }
    Ok(Value::from_node(node.into()))
}
