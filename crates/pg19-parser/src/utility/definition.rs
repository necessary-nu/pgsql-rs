//! Trigger and constraint reductions that share PostgreSQL constraint attributes.
use super::{constraint, foreign_key, trigger};
use crate::{value::Value, Failure};
use pg19_engine::Reduction;
use pg19_nodes::Node;

pub(crate) enum Definition {
    Integer(i32),
    Properties,
    MatchPartial,
    KeyAction(u8, bool),
    KeyActions(Option<isize>, Option<isize>),
    KeyUpdate,
    ForeignKey(bool),
    Event(i16, bool),
    MoreEvents,
    Trigger(bool),
    TriggerInt,
}

impl Definition {
    pub(super) fn reduce(self, rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
        match self {
            Self::Integer(value) => Ok(Value::Integer(value)),
            Self::Event(mask, has_columns) => Ok(Value::TriggerEvents(
                mask,
                if has_columns {
                    rhs.take(3).list()?
                } else {
                    vec![]
                },
            )),
            Self::MoreEvents => trigger::merge(rhs),
            Self::Trigger(constraint) => trigger::create(rhs, constraint),
            Self::TriggerInt => Ok(Value::Node(Node::String(
                rhs.take(1).integer()?.to_string(),
            ))),
            Self::MatchPartial => Err(Failure::At(
                "MATCH PARTIAL not yet implemented".to_owned(),
                "0A000",
                rhs.location(1),
            )),
            Self::KeyAction(kind, cols) => Ok(Value::KeyAction(
                kind,
                if cols {
                    rhs.take(3).list()?
                } else {
                    Vec::new()
                },
            )),
            Self::KeyActions(update, delete) => foreign_key::actions(rhs, update, delete),
            Self::KeyUpdate => foreign_key::update(rhs),
            Self::ForeignKey(table) => foreign_key::foreign_key(rhs, table),
            Self::Properties => constraint::combine(rhs),
        }
    }
}
