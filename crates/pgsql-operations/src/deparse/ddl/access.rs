use super::{behavior, ident, literal, named, names, required, role, session, Error, Plan, Result};
use pgsql_nodes::*;

pub(super) fn role_list(roles: &[Node], p: &mut Plan<'_>) -> Result<()> {
    if roles.is_empty() {
        return Err(Error::InvalidField("role list"));
    }
    for (i, item) in roles.iter().enumerate() {
        let Node::RoleSpec(spec) = item else {
            return Err(Error::InvalidField("role list"));
        };
        if i > 0 {
            p.text(", ");
        }
        p.mark(spec.location);
        p.text(role(spec)?);
    }
    Ok(())
}

fn option(node: &Node) -> Result<(&str, &DefElem)> {
    let Node::DefElem(d) = node else {
        return Err(Error::InvalidField("role option"));
    };
    Ok((
        d.defname
            .as_deref()
            .ok_or(Error::InvalidField("DefElem.defname"))?,
        d,
    ))
}

fn attribute(name: &str) -> Result<&'static str> {
    match name {
        "superuser" => Ok("SUPERUSER"),
        "createrole" => Ok("CREATEROLE"),
        "createdb" => Ok("CREATEDB"),
        "canlogin" => Ok("LOGIN"),
        "isreplication" => Ok("REPLICATION"),
        "bypassrls" => Ok("BYPASSRLS"),
        "inherit" => Ok("INHERIT"),
        _ => Err(Error::InvalidField("role attribute")),
    }
}

fn options<'a>(nodes: &'a [Node], p: &mut Plan<'a>) -> Result<()> {
    for (i, node) in nodes.iter().enumerate() {
        let (name, d) = option(node)?;
        p.text(if i == 0 { " WITH " } else { " " });
        p.mark(d.location);
        match (name, d.arg.as_deref()) {
            ("password", None) => p.text("PASSWORD NULL"),
            ("password", Some(arg @ Node::ParamRef(_))) => {
                p.text("PASSWORD ");
                p.expr(arg);
            }
            ("password" | "validUntil", Some(Node::String(s))) => {
                p.text(if name == "password" {
                    "PASSWORD "
                } else {
                    "VALID UNTIL "
                });
                p.text(literal(s)?);
            }
            ("connectionlimit" | "sysid", Some(Node::Integer(n))) => {
                p.text(if name == "sysid" {
                    "SYSID "
                } else {
                    "CONNECTION LIMIT "
                });
                p.text(n.to_string());
            }
            ("adminmembers" | "rolemembers" | "addroleto", Some(Node::List(list))) => {
                p.text(match name {
                    "adminmembers" => "ADMIN ",
                    "rolemembers" => "ROLE ",
                    _ => "IN ROLE ",
                });
                role_list(list, p)?;
            }
            (_, Some(Node::Boolean(enabled))) => {
                if !enabled {
                    p.text("NO");
                }
                p.text(attribute(name)?);
            }
            _ => return Err(Error::InvalidField("role option argument")),
        }
    }
    Ok(())
}

fn create<'a>(s: &'a CreateRoleStmt, p: &mut Plan<'a>) -> Result<()> {
    p.text(match s.stmt_type {
        RoleStmtType::ROLESTMT_ROLE => "CREATE ROLE ",
        RoleStmtType::ROLESTMT_USER => "CREATE USER ",
        RoleStmtType::ROLESTMT_GROUP => "CREATE GROUP ",
        _ => return Err(Error::InvalidField("CreateRoleStmt.stmt_type")),
    });
    p.text(named(&s.role, "CreateRoleStmt.role")?);
    options(&s.options, p)
}

fn alter<'a>(s: &'a AlterRoleStmt, p: &mut Plan<'a>) -> Result<()> {
    let who = role(required(&s.role, "AlterRoleStmt.role")?)?;
    if let [single] = &s.options[..] {
        let (name, d) = option(single)?;
        if name == "rolemembers" {
            let Some(Node::List(members)) = d.arg.as_deref() else {
                return Err(Error::InvalidField("AlterRoleStmt.options"));
            };
            p.text("ALTER GROUP ");
            p.text(who);
            p.text(match s.action {
                1 => " ADD USER ",
                -1 => " DROP USER ",
                _ => return Err(Error::InvalidField("AlterRoleStmt.action")),
            });
            return role_list(members, p);
        }
    }
    p.text("ALTER ROLE ");
    p.text(who);
    options(&s.options, p)
}

fn alter_setting<'a>(s: &'a AlterRoleSetStmt, p: &mut Plan<'a>) -> Result<()> {
    p.text("ALTER ROLE ");
    match &s.role {
        Some(who) => p.text(role(who)?),
        None => p.text("ALL"),
    }
    if let Some(db) = &s.database {
        p.text(" IN DATABASE ");
        p.text(ident(db)?);
    }
    p.text(" ");
    session::setting(required(&s.setstmt, "AlterRoleSetStmt.setstmt")?, p)
}

fn grant_option(node: &Node) -> Result<(&'static str, bool)> {
    let (name, d) = option(node)?;
    let word = match name {
        "admin" => "ADMIN",
        "inherit" => "INHERIT",
        "set" => "SET",
        _ => return Err(Error::InvalidField("membership option")),
    };
    let Some(Node::Boolean(enabled)) = d.arg.as_deref() else {
        return Err(Error::InvalidField("membership option argument"));
    };
    Ok((word, *enabled))
}

fn granted(roles: &[Node], p: &mut Plan<'_>) -> Result<()> {
    if roles.is_empty() {
        return Err(Error::InvalidField("GrantRoleStmt.granted_roles"));
    }
    for (i, n) in roles.iter().enumerate() {
        let Node::AccessPriv(a) = n else {
            return Err(Error::InvalidField("GrantRoleStmt.granted_roles"));
        };
        if i > 0 {
            p.text(", ");
        }
        p.text(match a.priv_name.as_deref() {
            Some(s @ ("select" | "references" | "create")) => s.to_owned(),
            Some(s) => ident(s)?,
            None => "ALL".to_owned(),
        });
        if !a.cols.is_empty() {
            p.text(" (");
            p.text(names(&a.cols, ", ")?);
            p.text(")");
        }
    }
    Ok(())
}

fn grant(s: &GrantRoleStmt, p: &mut Plan<'_>) -> Result<()> {
    p.text(if s.is_grant { "GRANT " } else { "REVOKE " });
    if !s.is_grant && !s.opt.is_empty() {
        let [only] = &s.opt[..] else {
            return Err(Error::InvalidField("GrantRoleStmt.opt"));
        };
        let (name, enabled) = grant_option(only)?;
        if enabled {
            return Err(Error::InvalidField("revoked membership option"));
        }
        p.text(name);
        p.text(" OPTION FOR ");
    }
    granted(&s.granted_roles, p)?;
    p.text(if s.is_grant { " TO " } else { " FROM " });
    role_list(&s.grantee_roles, p)?;
    if s.is_grant {
        for (i, node) in s.opt.iter().enumerate() {
            let (name, enabled) = grant_option(node)?;
            p.text(if i == 0 { " WITH " } else { ", " });
            p.text(name);
            p.text(if enabled { " OPTION" } else { " FALSE" });
        }
    }
    if let Some(who) = &s.grantor {
        p.text(" GRANTED BY ");
        p.text(role(who)?);
    }
    behavior(s.behavior, p)
}

pub(super) fn statement<'a>(n: &'a Node, p: &mut Plan<'a>) -> Result<bool> {
    match n {
        Node::CreateRoleStmt(s) => create(s, p)?,
        Node::AlterRoleStmt(s) => alter(s, p)?,
        Node::GrantRoleStmt(s) => grant(s, p)?,
        Node::AlterRoleSetStmt(s) => alter_setting(s, p)?,
        Node::DropRoleStmt(s) => {
            p.text(if s.missing_ok {
                "DROP ROLE IF EXISTS "
            } else {
                "DROP ROLE "
            });
            role_list(&s.roles, p)?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}
