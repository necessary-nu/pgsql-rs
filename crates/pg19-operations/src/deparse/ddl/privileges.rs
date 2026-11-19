use super::{behavior, object, role};
use crate::deparse::query::relation;
use crate::deparse::writer::{names, Plan};
use crate::deparse::{ident, numeric, Error, Result};
use pg19_nodes::*;

fn prefix(kind: ObjectType, target: GrantTargetType) -> Result<&'static str> {
    use GrantTargetType as T;
    use ObjectType as O;
    let prefix = match target {
        T::ACL_TARGET_OBJECT => match kind {
            O::OBJECT_TABLE => "",
            O::OBJECT_SEQUENCE => "SEQUENCE ",
            O::OBJECT_FDW => "FOREIGN DATA WRAPPER ",
            O::OBJECT_FOREIGN_SERVER => "FOREIGN SERVER ",
            O::OBJECT_FUNCTION => "FUNCTION ",
            O::OBJECT_PROCEDURE => "PROCEDURE ",
            O::OBJECT_ROUTINE => "ROUTINE ",
            O::OBJECT_DATABASE => "DATABASE ",
            O::OBJECT_DOMAIN => "DOMAIN ",
            O::OBJECT_LANGUAGE => "LANGUAGE ",
            O::OBJECT_LARGEOBJECT => "LARGE OBJECT ",
            O::OBJECT_PARAMETER_ACL => "PARAMETER ",
            O::OBJECT_SCHEMA => "SCHEMA ",
            O::OBJECT_TABLESPACE => "TABLESPACE ",
            O::OBJECT_TYPE => "TYPE ",
            _ => return Err(Error::InvalidField("GrantStmt.objtype")),
        },
        T::ACL_TARGET_ALL_IN_SCHEMA => match kind {
            O::OBJECT_TABLE => "ALL TABLES IN SCHEMA ",
            O::OBJECT_SEQUENCE => "ALL SEQUENCES IN SCHEMA ",
            O::OBJECT_FUNCTION => "ALL FUNCTIONS IN SCHEMA ",
            O::OBJECT_PROCEDURE => "ALL PROCEDURES IN SCHEMA ",
            O::OBJECT_ROUTINE => "ALL ROUTINES IN SCHEMA ",
            _ => return Err(Error::InvalidField("GrantStmt.objtype")),
        },
        T::ACL_TARGET_DEFAULTS => match kind {
            O::OBJECT_TABLE => "TABLES",
            O::OBJECT_FUNCTION => "FUNCTIONS",
            O::OBJECT_SEQUENCE => "SEQUENCES",
            O::OBJECT_TYPE => "TYPES",
            O::OBJECT_SCHEMA => "SCHEMAS",
            O::OBJECT_LARGEOBJECT => "LARGE OBJECTS",
            _ => return Err(Error::InvalidField("GrantStmt.objtype")),
        },
        _ => return Err(Error::InvalidField("GrantStmt.targtype")),
    };
    Ok(prefix)
}

pub(super) fn grant<'a>(s: &'a GrantStmt, p: &mut Plan<'a>) -> Result<()> {
    p.text(if s.is_grant { "GRANT " } else { "REVOKE " });
    if !s.is_grant && s.grant_option {
        p.text("GRANT OPTION FOR ");
    }
    if s.privileges.is_empty() {
        p.text("ALL");
    }
    for (i, n) in s.privileges.iter().enumerate() {
        if i > 0 {
            p.text(", ");
        }
        let Node::AccessPriv(a) = n else {
            return Err(Error::InvalidField("GrantStmt.privileges"));
        };
        match a.priv_name.as_deref() {
            None => p.text("ALL"),
            Some(name @ ("select" | "references" | "create")) => p.text(name),
            Some(name) => p.text(ident(name)?),
        }
        if !a.cols.is_empty() {
            p.text(" (");
            p.text(names(&a.cols, ", ")?);
            p.text(")");
        }
    }
    p.text(" ON ");
    p.text(prefix(s.objtype, s.targtype)?);
    for (i, n) in s.objects.iter().enumerate() {
        if i > 0 {
            p.text(", ");
        }
        match n {
            Node::RangeVar(r) => relation(r, p, false)?,
            Node::ObjectWithArgs(a) => object::signature(s.objtype, a, p)?,
            Node::String(name) => p.text(ident(name)?),
            Node::List(list) => p.text(names(list, ".")?),
            Node::Integer(i) => p.text(i.to_string()),
            Node::Float(f) => p.text(numeric(f)?),
            _ => return Err(Error::InvalidField("GrantStmt.objects")),
        }
    }
    p.text(if s.is_grant { " TO " } else { " FROM " });
    for (i, n) in s.grantees.iter().enumerate() {
        if i > 0 {
            p.text(", ");
        }
        let Node::RoleSpec(r) = n else {
            return Err(Error::InvalidField("GrantStmt.grantees"));
        };
        p.text(role(r)?);
    }
    if s.is_grant && s.grant_option {
        p.text(" WITH GRANT OPTION");
    }
    if let Some(r) = &s.grantor {
        p.text(" GRANTED BY ");
        p.text(role(r)?);
    }
    behavior(s.behavior, p)?;
    Ok(())
}
