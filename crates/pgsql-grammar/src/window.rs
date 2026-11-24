//! Window frames and their SQLSTATE 42P20 checks from PostgreSQL gram.y.
use super::{value::Value, Failure};
use crate::raw::*;
use pgsql_engine::Reduction;

// PostgreSQL parsenodes.h pairs each START bit with its END bit one position higher.
const BETWEEN: i32 = 0x10;
const UNBOUNDED_PRECEDING: i32 = 0x20;
const UNBOUNDED_FOLLOWING: i32 = 0x80;
const CURRENT: i32 = 0x200;
const PRECEDING: i32 = 0x800;
const FOLLOWING: i32 = 0x2000;
pub(super) const DEFAULTS: i32 = 2 | UNBOUNDED_PRECEDING | (CURRENT << 1);

pub(super) enum Window {
    Bound(i32, bool),
    Extent(bool),
    Mode(i32),
    Default,
    Specification,
}

fn window(value: Value) -> Result<NodeBox<WindowDef>, Failure> {
    match value {
        Value::Node(Node::WindowDef(w)) => Ok(w),
        _ => Err("expected window frame".into()),
    }
}

fn extent(between: bool, rhs: &mut Reduction<'_, Value>) -> Result<NodeBox<WindowDef>, Failure> {
    let start = if between { 2 } else { 1 };
    let mut bounds = window(rhs.take(start))?;
    let (end, offset) = if between {
        let mut bound = window(rhs.take(4))?;
        (bound.frame_options, bound.start_offset.take())
    } else {
        (CURRENT, None)
    };
    let invalid = match (bounds.frame_options, end, between) {
        (UNBOUNDED_FOLLOWING, _, _) => Some(("frame start cannot be UNBOUNDED FOLLOWING", start)),
        (_, UNBOUNDED_PRECEDING, true) => Some(("frame end cannot be UNBOUNDED PRECEDING", 4)),
        (FOLLOWING, _, false) => Some((
            "frame starting from following row cannot end with current row",
            1,
        )),
        (CURRENT, PRECEDING, true) => Some((
            "frame starting from current row cannot have preceding rows",
            4,
        )),
        (FOLLOWING, PRECEDING | CURRENT, true) => Some((
            "frame starting from following row cannot have preceding rows",
            4,
        )),
        _ => None,
    };
    if let Some((msg, i)) = invalid {
        return Err(Failure::At(msg.into(), "42P20", rhs.location(i)));
    }
    bounds.frame_options |= end << 1;
    if between {
        bounds.frame_options |= BETWEEN;
    }
    bounds.end_offset = offset;
    Ok(bounds)
}

#[cfg_attr(not(debug_assertions), inline(always))]
pub(super) fn reduce(op: Window, rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let window: NodeBox<WindowDef> = match op {
        Window::Bound(bits, offset) => WindowDef {
            frame_options: bits,
            start_offset: if offset {
                Some(Box::new(rhs.take(1).node()?))
            } else {
                None
            },
            ..Default::default()
        }
        .into(),
        Window::Default => WindowDef {
            frame_options: DEFAULTS,
            ..Default::default()
        }
        .into(),
        Window::Extent(between) => extent(between, rhs)?,
        Window::Mode(bits) => {
            let mut frame = window(rhs.take(2))?;
            frame.frame_options |= 1 | bits | rhs.take(3).integer()?;
            frame
        }
        Window::Specification => {
            let mut frame = window(rhs.take(5))?;
            frame.refname = crate::records::optional_text(rhs.take(2))?;
            frame.partition_clause = rhs.take(3).list()?;
            frame.order_clause = rhs.take(4).list()?;
            frame.location = rhs.location(1);
            frame
        }
    };
    Ok(Value::from_node(window.into()))
}
