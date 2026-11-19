//! PostgreSQL utility grammar actions and their shared relation-name reductions.
use super::{value::Value, Failure};
use pg19_engine::Reduction;
use pg19_nodes::*;

pub(super) enum Utility {
    Qualified(bool),
    Relation(isize, bool),
}

fn qualified(rhs: &mut Reduction<'_, Value>, dotted: bool) -> Result<Value, Failure> {
    let first = rhs.take(1).text()?;
    let location = rhs.location(1);
    let mut parts = vec![first];
    if dotted {
        for value in rhs.take(2).list()? {
            let Node::String(name) = value else {
                return Err(Failure::Syntax("syntax error"));
            };
            parts.push(name);
        }
    }
    if parts.len() > 3 {
        return Err(Failure::At(
            format!(
                "improper qualified name (too many dotted names): {}",
                parts.join(".")
            ),
            "42601",
            location,
        ));
    }
    Ok(Value::Node(
        RangeVar {
            relname: parts.pop(),
            schemaname: parts.pop(),
            catalogname: parts.pop(),
            relpersistence: b'p',
            inh: true,
            location,
            ..Default::default()
        }
        .into(),
    ))
}

pub(super) fn reduce(op: Utility, rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    match op {
        Utility::Qualified(dotted) => qualified(rhs, dotted),
        Utility::Relation(index, inherit) => {
            let Node::RangeVar(mut range) = rhs.take(index).node()? else {
                return Err("expected relation name".into());
            };
            range.inh = inherit;
            range.alias = None;
            Ok(Value::Node(Node::RangeVar(range)))
        }
    }
}
