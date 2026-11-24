//! Logical replication statements, including PostgreSQL 19 publication and server forms.
use super::{behavior, index};
use crate::deparse::query::{named, relation};
use crate::deparse::writer::{names, required, Plan};
use crate::deparse::{literal, Error, Result};
use pgsql_nodes::*;

pub(super) fn definition<'a>(opts: &'a [Node], p: &mut Plan<'a>) -> Result<()> {
    if opts.is_empty() {
        return Err(Error::InvalidField("definition"));
    }
    p.text("(");
    for (i, n) in opts.iter().enumerate() {
        let Node::DefElem(d) = n else {
            return Err(Error::InvalidField("definition"));
        };
        if i > 0 {
            p.text(", ")
        }
        p.text(named(&d.defname, "DefElem.defname")?);
        if let Some(arg) = &d.arg {
            p.text(" = ");
            index::option_value(arg, p)?;
        }
    }
    p.text(")");
    Ok(())
}

fn with_options<'a>(opts: &'a [Node], p: &mut Plan<'a>) -> Result<()> {
    if !opts.is_empty() {
        p.text(" WITH ");
        definition(opts, p)?;
    }
    Ok(())
}

fn publication_objects<'a>(
    objects: &'a [Node],
    all_tables: bool,
    all_sequences: bool,
    p: &mut Plan<'a>,
) -> Result<()> {
    use PublicationObjSpecType as T;
    if all_tables {
        p.text("ALL TABLES");
        if !objects.is_empty() {
            p.text(" EXCEPT (")
        }
    }
    for (i, n) in objects.iter().enumerate() {
        let Node::PublicationObjSpec(o) = n else {
            return Err(Error::InvalidField("publication objects"));
        };
        if i > 0 {
            p.text(", ")
        }
        match (all_tables, o.pubobjtype) {
            (true, T::PUBLICATIONOBJ_EXCEPT_TABLE) | (false, T::PUBLICATIONOBJ_TABLE) => {
                let table = required(&o.pubtable, "PublicationObjSpec.pubtable")?;
                p.text("TABLE ");
                relation(
                    required(&table.relation, "PublicationTable.relation")?,
                    p,
                    false,
                )?;
                if !table.columns.is_empty() {
                    p.text("(");
                    p.text(names(&table.columns, ", ")?);
                    p.text(")");
                }
                if let Some(w) = &table.where_clause {
                    p.text(" WHERE (");
                    p.expr(w);
                    p.text(")");
                }
            }
            (false, T::PUBLICATIONOBJ_TABLES_IN_SCHEMA) => {
                p.text("TABLES IN SCHEMA ");
                p.text(named(&o.name, "PublicationObjSpec.name")?);
            }
            (false, T::PUBLICATIONOBJ_TABLES_IN_CUR_SCHEMA) => {
                p.text("TABLES IN SCHEMA CURRENT_SCHEMA")
            }
            _ => return Err(Error::InvalidField("PublicationObjSpec.pubobjtype")),
        }
    }
    if all_tables && !objects.is_empty() {
        p.text(")")
    }
    if all_sequences {
        if all_tables {
            p.text(", ")
        }
        p.text("ALL SEQUENCES");
    }
    Ok(())
}

fn alter_publication<'a>(s: &'a AlterPublicationStmt, p: &mut Plan<'a>) -> Result<()> {
    use AlterPublicationAction as A;
    p.text("ALTER PUBLICATION ");
    p.text(named(&s.pubname, "AlterPublicationStmt.pubname")?);
    if !s.options.is_empty() {
        p.text(" SET ");
        return definition(&s.options, p);
    }
    p.text(match s.action {
        A::AP_AddObjects => " ADD ",
        A::AP_DropObjects => " DROP ",
        A::AP_SetObjects => " SET ",
        _ => return Err(Error::InvalidField("AlterPublicationStmt.action")),
    });
    publication_objects(&s.pubobjects, s.for_all_tables, s.for_all_sequences, p)
}

fn connection<'a>(conninfo: Option<&str>, server: &Option<String>, p: &mut Plan<'a>) -> Result<()> {
    if server.is_some() {
        p.text(" SERVER ");
        p.text(named(server, "subscription server")?);
    } else {
        p.text(" CONNECTION ");
        p.text(literal(conninfo.unwrap_or(""))?);
    }
    Ok(())
}

fn alter_subscription<'a>(s: &'a AlterSubscriptionStmt, p: &mut Plan<'a>) -> Result<()> {
    use AlterSubscriptionType as T;
    p.text("ALTER SUBSCRIPTION ");
    p.text(named(&s.subname, "AlterSubscriptionStmt.subname")?);
    match s.kind {
        T::ALTER_SUBSCRIPTION_OPTIONS | T::ALTER_SUBSCRIPTION_SKIP => {
            p.text(if s.kind == T::ALTER_SUBSCRIPTION_SKIP {
                " SKIP "
            } else {
                " SET "
            });
            return definition(&s.options, p);
        }
        T::ALTER_SUBSCRIPTION_SERVER | T::ALTER_SUBSCRIPTION_CONNECTION => {
            return connection(s.conninfo.as_deref(), &s.servername, p)
        }
        T::ALTER_SUBSCRIPTION_REFRESH_PUBLICATION => p.text(" REFRESH PUBLICATION"),
        T::ALTER_SUBSCRIPTION_REFRESH_SEQUENCES => p.text(" REFRESH SEQUENCES"),
        T::ALTER_SUBSCRIPTION_ADD_PUBLICATION
        | T::ALTER_SUBSCRIPTION_DROP_PUBLICATION
        | T::ALTER_SUBSCRIPTION_SET_PUBLICATION => {
            p.text(match s.kind {
                T::ALTER_SUBSCRIPTION_ADD_PUBLICATION => " ADD PUBLICATION ",
                T::ALTER_SUBSCRIPTION_DROP_PUBLICATION => " DROP PUBLICATION ",
                _ => " SET PUBLICATION ",
            });
            p.text(names(&s.publication, ", ")?);
        }
        T::ALTER_SUBSCRIPTION_ENABLED => {
            let [Node::DefElem(d)] = &s.options[..] else {
                return Err(Error::InvalidField("AlterSubscriptionStmt.options"));
            };
            if d.defname.as_deref() != Some("enabled") {
                return Err(Error::InvalidField("DefElem.defname"));
            }
            p.text(match d.arg.as_deref() {
                Some(Node::Boolean(true)) => "  ENABLE",
                Some(Node::Boolean(false)) => "  DISABLE",
                _ => return Err(Error::InvalidField("DefElem.arg")),
            });
            return Ok(());
        }
        _ => return Err(Error::InvalidField("AlterSubscriptionStmt.kind")),
    }
    with_options(&s.options, p)
}

pub(super) fn statement<'a>(n: &'a Node, p: &mut Plan<'a>) -> Result<bool> {
    match n {
        Node::CreatePublicationStmt(s) => {
            p.text("CREATE PUBLICATION ");
            p.text(named(&s.pubname, "CreatePublicationStmt.pubname")?);
            if s.for_all_tables || s.for_all_sequences || !s.pubobjects.is_empty() {
                p.text(" FOR ");
                publication_objects(&s.pubobjects, s.for_all_tables, s.for_all_sequences, p)?;
            }
            with_options(&s.options, p)?;
        }
        Node::AlterPublicationStmt(s) => alter_publication(s, p)?,
        Node::CreateSubscriptionStmt(s) => {
            p.text("CREATE SUBSCRIPTION ");
            p.text(named(&s.subname, "CreateSubscriptionStmt.subname")?);
            connection(s.conninfo.as_deref(), &s.servername, p)?;
            p.text(" PUBLICATION ");
            p.text(names(&s.publication, ", ")?);
            with_options(&s.options, p)?;
        }
        Node::AlterSubscriptionStmt(s) => alter_subscription(s, p)?,
        Node::DropSubscriptionStmt(s) => {
            p.text("DROP SUBSCRIPTION ");
            if s.missing_ok {
                p.text("IF EXISTS ")
            }
            p.text(named(&s.subname, "DropSubscriptionStmt.subname")?);
            behavior(s.behavior, p)?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}
