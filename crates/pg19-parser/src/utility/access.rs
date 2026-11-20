//! Role option and privilege records from PostgreSQL gram.y.
use crate::{
    records::record,
    value::{OwnedValue, Value},
    Failure,
};
use pg19_engine::Reduction;
use pg19_nodes::*;

pub(crate) struct PrivilegeTarget {
    pub kind: GrantTargetType,
    pub object_type: ObjectType,
    pub objects: Vec<Node>,
}

pub(super) fn grant(
    rhs: &mut Reduction<'_, Value>,
    is_grant: bool,
    option_for: bool,
) -> Result<Value, Failure> {
    let skip = if option_for { 3 } else { 0 };
    let Value::Owned(OwnedValue::PrivilegeTarget(target)) = rhs.take(4 + skip) else {
        return Err("expected privilege target".into());
    };
    let grant_option = if is_grant {
        rhs.take(7).integer()? != 0
    } else {
        option_for
    };
    let grantor = record!(rhs.take(if is_grant { 8 } else { 7 + skip }), RoleSpec);
    let behavior = if is_grant {
        DropBehavior::default()
    } else {
        DropBehavior(rhs.take(8 + skip).integer()?)
    };
    Ok(Value::from_node(
        GrantStmt {
            is_grant,
            targtype: target.kind,
            objtype: target.object_type,
            objects: target.objects,
            privileges: rhs.take(2 + skip).list()?,
            grantees: rhs.take(6 + skip).list()?,
            grant_option,
            grantor,
            behavior,
        }
        .into(),
    ))
}

pub(super) fn role_option(rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let word = rhs.take(1).text()?;
    let location = rhs.location(1);
    let (role, enabled) = match word.strip_prefix("no") {
        Some(role) => (role, false),
        None => (word.as_str(), true),
    };
    let defname = match role {
        "superuser" | "createrole" | "createdb" | "bypassrls" => role,
        "login" => "canlogin",
        "replication" => "isreplication",
        "inherit" if !enabled => "inherit",
        _ => {
            return Err(Failure::At(
                format!("unrecognized role option \"{word}\""),
                "42601",
                location,
            ))
        }
    };
    Ok(Value::from_node(
        DefElem {
            defname: Some(defname.to_owned()),
            arg: Some(Box::new(Node::Boolean(enabled))),
            location,
            arg_location: Location::UNKNOWN,
            ..Default::default()
        }
        .into(),
    ))
}
