use super::{literal, named, names, relation_list, required, role, Error, Plan, Result};
use pgsql_nodes::*;

pub(super) fn options(items: &[Node], p: &mut Plan<'_>) -> Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    p.text(" OPTIONS (");
    for (i, node) in items.iter().enumerate() {
        let Node::DefElem(d) = node else {
            return Err(Error::InvalidField("foreign options"));
        };
        if i > 0 {
            p.text(", ");
        }
        p.mark(d.location);
        p.text(match d.defaction {
            DefElemAction::DEFELEM_UNSPEC => "",
            DefElemAction::DEFELEM_ADD => "ADD ",
            DefElemAction::DEFELEM_SET => "SET ",
            DefElemAction::DEFELEM_DROP => "DROP ",
            _ => return Err(Error::InvalidField("DefElem.defaction")),
        });
        p.text(named(&d.defname, "DefElem.defname")?);
        match (d.defaction, d.arg.as_deref()) {
            (DefElemAction::DEFELEM_DROP, None) => {}
            (_, Some(Node::String(s))) if d.defaction != DefElemAction::DEFELEM_DROP => {
                p.text(" ");
                p.text(literal(s)?);
            }
            _ => return Err(Error::InvalidField("foreign option argument")),
        }
    }
    p.text(")");
    Ok(())
}

fn wrapper(
    name: &Option<String>,
    handlers: &[Node],
    args: &[Node],
    p: &mut Plan<'_>,
) -> Result<()> {
    p.text(named(name, "foreign data wrapper name")?);
    for (i, node) in handlers.iter().enumerate() {
        let Node::DefElem(d) = node else {
            return Err(Error::InvalidField("FDW handler options"));
        };
        let word = match d.defname.as_deref() {
            Some("handler") => "HANDLER",
            Some("validator") => "VALIDATOR",
            Some("connection") => "CONNECTION",
            _ => return Err(Error::InvalidField("FDW handler name")),
        };
        p.text(if d.arg.is_some() { " " } else { " NO " });
        p.text(word);
        if let Some(arg) = &d.arg {
            let Node::List(function) = arg.as_ref() else {
                return Err(Error::InvalidField("FDW handler"));
            };
            if function.is_empty() {
                return Err(Error::InvalidField("FDW handler"));
            }
            p.text(" ");
            p.text(names(function, ".")?);
        } else if i + 1 < handlers.len() || !args.is_empty() {
            p.text(" ");
        }
    }
    options(args, p)
}

fn mapping(
    user: &Option<Box<RoleSpec>>,
    server: &Option<String>,
    args: &[Node],
    p: &mut Plan<'_>,
) -> Result<()> {
    p.text("FOR ");
    let user = required(user, "user mapping role")?;
    p.text(role(user)?);
    p.text(" SERVER ");
    p.text(named(server, "user mapping server")?);
    options(args, p)
}

fn import(s: &ImportForeignSchemaStmt, p: &mut Plan<'_>) -> Result<()> {
    p.text("IMPORT FOREIGN SCHEMA ");
    p.text(named(
        &s.remote_schema,
        "ImportForeignSchemaStmt.remote_schema",
    )?);
    if s.list_type != ImportForeignSchemaType::FDW_IMPORT_SCHEMA_ALL {
        p.text(match s.list_type {
            ImportForeignSchemaType::FDW_IMPORT_SCHEMA_LIMIT_TO => " LIMIT TO (",
            ImportForeignSchemaType::FDW_IMPORT_SCHEMA_EXCEPT => " EXCEPT (",
            _ => return Err(Error::InvalidField("ImportForeignSchemaStmt.list_type")),
        });
        relation_list(&s.table_list, p)?;
        p.text(")");
    }
    p.text(" FROM SERVER ");
    p.text(named(
        &s.server_name,
        "ImportForeignSchemaStmt.server_name",
    )?);
    p.text(" INTO ");
    p.text(named(
        &s.local_schema,
        "ImportForeignSchemaStmt.local_schema",
    )?);
    options(&s.options, p)
}

pub(super) fn statement(n: &Node, p: &mut Plan<'_>) -> Result<bool> {
    match n {
        Node::CreateFdwStmt(s) => {
            p.text("CREATE FOREIGN DATA WRAPPER ");
            wrapper(&s.fdwname, &s.func_options, &s.options, p)?;
        }
        Node::AlterFdwStmt(s) => {
            p.text("ALTER FOREIGN DATA WRAPPER ");
            wrapper(&s.fdwname, &s.func_options, &s.options, p)?;
        }
        Node::CreateForeignServerStmt(s) => {
            p.text(if s.if_not_exists {
                "CREATE SERVER IF NOT EXISTS "
            } else {
                "CREATE SERVER "
            });
            p.text(named(&s.servername, "CreateForeignServerStmt.servername")?);
            if let Some(kind) = &s.servertype {
                p.text(" TYPE ");
                p.text(literal(kind)?);
            }
            if let Some(version) = &s.version {
                p.text(" VERSION ");
                p.text(literal(version)?);
            }
            p.text(" FOREIGN DATA WRAPPER ");
            p.text(named(&s.fdwname, "CreateForeignServerStmt.fdwname")?);
            options(&s.options, p)?;
        }
        Node::AlterForeignServerStmt(s) => {
            p.text("ALTER SERVER ");
            p.text(named(&s.servername, "AlterForeignServerStmt.servername")?);
            if s.has_version {
                p.text(" VERSION ");
                match &s.version {
                    Some(v) => p.text(literal(v)?),
                    None => p.text("NULL"),
                }
            }
            options(&s.options, p)?;
        }
        Node::CreateUserMappingStmt(s) => {
            p.text(if s.if_not_exists {
                "CREATE USER MAPPING IF NOT EXISTS "
            } else {
                "CREATE USER MAPPING "
            });
            mapping(&s.user, &s.servername, &s.options, p)?;
        }
        Node::AlterUserMappingStmt(s) => {
            p.text("ALTER USER MAPPING ");
            mapping(&s.user, &s.servername, &s.options, p)?;
        }
        Node::DropUserMappingStmt(s) => {
            p.text(if s.missing_ok {
                "DROP USER MAPPING IF EXISTS "
            } else {
                "DROP USER MAPPING "
            });
            mapping(&s.user, &s.servername, &[], p)?;
        }
        Node::ImportForeignSchemaStmt(s) => import(s, p)?,
        _ => return Ok(false),
    }
    Ok(true)
}
