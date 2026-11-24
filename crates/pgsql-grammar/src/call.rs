//! Function calls, aggregate clauses and sort items from PG19 gram.y.
use super::{value::Value, Failure};
use crate::raw::*;
use pgsql_engine::Reduction;

pub(super) enum Call {
    Name(bool),
    Apply {
        args: Option<isize>,
        variadic: Option<isize>,
        order: Option<isize>,
        distinct: bool,
        star: bool,
    },
    NamedArgument,
    Aggregate,
    Sort,
    UsingSort,
    NamedWindow,
}

pub(super) fn function_name(
    qualified: bool,
    rhs: &mut Reduction<'_, Value>,
) -> Result<Value, Failure> {
    let name = Node::String(rhs.take(1).text()?);
    let names = if qualified {
        let mut tail = match rhs.take(2) {
            Value::Node(field @ Node::String(_)) => return Ok(Value::from_list(vec![name, field])),
            Value::Node(_) => return Err(Failure::Syntax("syntax error")),
            fields => fields.list()?,
        };
        if tail.iter().any(|n| !matches!(n, Node::String(_))) {
            return Err(Failure::Syntax("syntax error"));
        }
        tail.insert(0, name);
        tail
    } else {
        vec![name]
    };
    Ok(Value::from_list(names))
}

#[inline]
pub(super) fn aggregate<const DECORATED: bool>(
    rhs: &mut Reduction<'_, Value>,
) -> Result<Value, Failure> {
    let Node::FuncCall(mut call) = rhs.take(1).node()? else {
        return Err("expected FuncCall".into());
    };
    if !DECORATED {
        call.agg_filter = None;
        call.ignore_nulls = 0;
        call.over = None;
        return Ok(Value::from_node(call.into()));
    }
    let within = rhs.take(2).list()?;
    if !within.is_empty() {
        let error = if !call.agg_order.is_empty() {
            Some("cannot use multiple ORDER BY clauses with WITHIN GROUP")
        } else if call.agg_distinct {
            Some("cannot use DISTINCT with WITHIN GROUP")
        } else if call.func_variadic {
            Some("cannot use VARIADIC with WITHIN GROUP")
        } else {
            None
        };
        if let Some(message) = error {
            return Err(Failure::At(message.into(), "42601", rhs.location(2)));
        }
        call.agg_order = within;
        call.agg_within_group = true;
    }
    call.agg_filter = rhs.take(3).optional()?;
    call.ignore_nulls = rhs.take(4).integer()?;
    call.over = match rhs.take(5) {
        Value::Empty => None,
        Value::Node(Node::WindowDef(w)) => Some(w.into_box()),
        _ => return Err("expected window definition".into()),
    };
    Ok(Value::from_node(call.into()))
}

#[cfg_attr(not(debug_assertions), inline(always))]
pub(super) fn reduce(op: Call, rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let node = match op {
        Call::Name(qualified) => return function_name(qualified, rhs),
        Call::Apply {
            args,
            variadic,
            order,
            distinct,
            star,
        } => {
            let mut args = args
                .map(|n| rhs.take(n).list())
                .transpose()?
                .unwrap_or_default();
            if let Some(n) = variadic {
                args.push(rhs.take(n).node()?);
            }
            FuncCall {
                funcname: rhs.take(1).list()?,
                args,
                agg_order: order
                    .map(|n| rhs.take(n).list())
                    .transpose()?
                    .unwrap_or_default(),
                agg_distinct: distinct,
                agg_star: star,
                func_variadic: variadic.is_some(),
                funcformat: CoercionForm::COERCE_EXPLICIT_CALL,
                location: rhs.location(1),
                ..Default::default()
            }
            .into()
        }
        Call::NamedArgument => NamedArgExpr {
            name: Some(rhs.take(1).text()?),
            arg: Some(Box::new(rhs.take(3).node()?)),
            argnumber: -1,
            location: rhs.location(1),
        }
        .into(),
        Call::Aggregate => return aggregate::<true>(rhs),
        Call::Sort => SortBy {
            node: Some(Box::new(rhs.take(1).node()?)),
            sortby_dir: SortByDir(rhs.take(2).integer()?),
            sortby_nulls: SortByNulls(rhs.take(3).integer()?),
            location: Location::UNKNOWN,
            ..Default::default()
        }
        .into(),
        Call::UsingSort => SortBy {
            node: Some(Box::new(rhs.take(1).node()?)),
            sortby_dir: SortByDir::SORTBY_USING,
            sortby_nulls: SortByNulls(rhs.take(4).integer()?),
            use_op: rhs.take(3).list()?,
            location: rhs.location(3),
        }
        .into(),
        Call::NamedWindow => WindowDef {
            name: Some(rhs.take(2).text()?),
            frame_options: crate::window::DEFAULTS,
            location: rhs.location(2),
            ..Default::default()
        }
        .into(),
    };
    Ok(Value::from_node(node))
}
