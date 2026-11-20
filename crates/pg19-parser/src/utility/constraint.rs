//! PostgreSQL's ConstraintAttributeSpec and processCASbits.
use crate::raw::{ConstrType, Constraint, Location};
use crate::{value::Value, Failure};
use pg19_engine::Reduction;

pub(crate) const CAS_NOT_DEFERRABLE: i32 = 0x01;
pub(crate) const CAS_DEFERRABLE: i32 = 0x02;
pub(crate) const CAS_INITIALLY_IMMEDIATE: i32 = 0x04;
pub(crate) const CAS_INITIALLY_DEFERRED: i32 = 0x08;
pub(crate) const CAS_NOT_VALID: i32 = 0x10;
pub(crate) const CAS_NO_INHERIT: i32 = 0x20;
pub(crate) const CAS_NOT_ENFORCED: i32 = 0x40;
pub(crate) const CAS_ENFORCED: i32 = 0x80;

pub(super) fn combine(rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let flags = rhs.take(1).integer()? | rhs.take(2).integer()?;
    let both = |a, b| flags & (a | b) == a | b;
    let error = if both(CAS_NOT_DEFERRABLE, CAS_INITIALLY_DEFERRED) {
        Some("constraint declared INITIALLY DEFERRED must be DEFERRABLE")
    } else if both(CAS_NOT_DEFERRABLE, CAS_DEFERRABLE)
        || both(CAS_INITIALLY_IMMEDIATE, CAS_INITIALLY_DEFERRED)
        || both(CAS_NOT_ENFORCED, CAS_ENFORCED)
    {
        Some("conflicting constraint properties")
    } else {
        None
    };
    if let Some(message) = error {
        return Err(Failure::At(message.to_owned(), "42601", rhs.location(2)));
    }
    Ok(Value::Integer(flags))
}

pub(crate) fn process(
    node: &mut Constraint,
    bits: i32,
    loc: Location,
    label: &str,
    allowed: i32,
) -> Result<(), Failure> {
    // C passes null output pointers for unsupported properties. Each check follows
    // PostgreSQL's diagnostic precedence, including its DEFERRABLE spelling.
    for (requested, destination, word) in [
        (
            CAS_DEFERRABLE | CAS_INITIALLY_DEFERRED,
            CAS_DEFERRABLE,
            "DEFERRABLE",
        ),
        (CAS_INITIALLY_DEFERRED, CAS_INITIALLY_DEFERRED, "DEFERRABLE"),
        (CAS_NOT_VALID, CAS_NOT_VALID, "NOT VALID"),
        (CAS_NO_INHERIT, CAS_NO_INHERIT, "NO INHERIT"),
        (CAS_NOT_ENFORCED, CAS_ENFORCED, "NOT ENFORCED"),
        (CAS_ENFORCED, CAS_ENFORCED, "ENFORCED"),
    ] {
        if bits & requested != 0 && allowed & destination == 0 {
            return Err(Failure::At(
                format!("{label} constraints cannot be marked {word}"),
                "0A000",
                loc,
            ));
        }
    }
    if allowed & CAS_DEFERRABLE != 0 {
        node.deferrable = bits & (CAS_DEFERRABLE | CAS_INITIALLY_DEFERRED) != 0;
    }
    if allowed & CAS_INITIALLY_DEFERRED != 0 {
        node.initdeferred = bits & CAS_INITIALLY_DEFERRED != 0;
    }
    if allowed & CAS_ENFORCED != 0 {
        node.is_enforced = bits & CAS_NOT_ENFORCED == 0;
    }
    if allowed & CAS_NOT_VALID != 0 {
        node.skip_validation = bits & (CAS_NOT_VALID | CAS_NOT_ENFORCED) != 0;
    }
    if bits & CAS_NO_INHERIT != 0 {
        node.is_no_inherit = true;
    }
    Ok(())
}

pub(super) fn generated_column(rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let when = rhs.take(2).integer()? as u8;
    if when != b'a' {
        return Err(Failure::At(
            "for a generated column, GENERATED ALWAYS must be specified".to_owned(),
            "42601",
            rhs.location(2),
        ));
    }
    Ok(Value::from_node(
        Constraint {
            contype: ConstrType::CONSTR_GENERATED,
            generated_when: when,
            raw_expr: rhs.take(5).optional()?,
            generated_kind: rhs.take(7).integer()? as u8,
            location: rhs.location(1),
            ..Default::default()
        }
        .into(),
    ))
}
