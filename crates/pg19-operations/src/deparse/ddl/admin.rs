//! Database and maintenance statements from PostgreSQL 19 gram.y and postgres_deparse.c.
use super::{index, role, session};
use crate::deparse::query::{named, relation};
use crate::deparse::writer::{names, required, Plan, Role};
use crate::deparse::{ident, literal, option_value, plain_name, utility_options, Error, Result};
use pg19_nodes::*;

fn database_options(options: &[Node], p: &mut Plan<'_>) -> Result<()> {
    for option in options {
        let Node::DefElem(d) = option else {
            return Err(Error::InvalidField("database options"));
        };
        let name = d
            .defname
            .as_deref()
            .ok_or(Error::InvalidField("DefElem.defname"))?;
        p.text(" ");
        // The grammar accepts these specific keyword tokens or an ordinary identifier.
        p.text(match name {
            "connection_limit" => "CONNECTION LIMIT".to_owned(),
            "encoding" | "location" | "owner" | "tablespace" | "template" => {
                name.to_ascii_uppercase()
            }
            _ if plain_name(name) && pg19_lexer::keyword(name).is_none() => {
                name.to_ascii_uppercase()
            }
            _ if plain_name(name) && name.len() <= 63 => format!("\"{name}\""),
            _ => ident(name)?,
        });
        p.text(" ");
        match d.arg.as_deref() {
            Some(v) => p.text(option_value(v)?),
            None => p.text("DEFAULT"),
        }
    }
    Ok(())
}

fn database<'a>(n: &'a Node, p: &mut Plan<'a>) -> Result<bool> {
    match n {
        Node::CreatedbStmt(s) => {
            p.text("CREATE DATABASE ");
            p.text(named(&s.dbname, "CreatedbStmt.dbname")?);
            database_options(&s.options, p)?;
        }
        Node::AlterDatabaseStmt(s) => {
            p.text("ALTER DATABASE ");
            p.text(named(&s.dbname, "AlterDatabaseStmt.dbname")?);
            database_options(&s.options, p)?;
        }
        Node::AlterDatabaseRefreshCollStmt(s) => {
            p.text("ALTER DATABASE ");
            p.text(named(&s.dbname, "AlterDatabaseRefreshCollStmt.dbname")?);
            p.text(" REFRESH COLLATION VERSION");
        }
        Node::AlterDatabaseSetStmt(s) => {
            p.text("ALTER DATABASE ");
            p.text(named(&s.dbname, "AlterDatabaseSetStmt.dbname")?);
            p.text(" ");
            session::setting(required(&s.setstmt, "AlterDatabaseSetStmt.setstmt")?, p)?;
        }
        Node::DropdbStmt(s) => {
            p.text("DROP DATABASE ");
            if s.missing_ok {
                p.text("IF EXISTS ")
            }
            p.text(named(&s.dbname, "DropdbStmt.dbname")?);
            if !s.options.is_empty() {
                p.text(" (");
                for (i, o) in s.options.iter().enumerate() {
                    if !matches!(o, Node::DefElem(d) if d.defname.as_deref()==Some("force") && d.arg.is_none())
                    {
                        return Err(Error::InvalidField("DropdbStmt.options"));
                    }
                    if i > 0 {
                        p.text(", ")
                    }
                    p.text("FORCE")
                }
                p.text(")")
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn move_all(s: &AlterTableMoveAllStmt, p: &mut Plan<'_>) -> Result<()> {
    p.text(match s.objtype {
        ObjectType::OBJECT_TABLE => "ALTER TABLE",
        ObjectType::OBJECT_INDEX => "ALTER INDEX",
        ObjectType::OBJECT_MATVIEW => "ALTER MATERIALIZED VIEW",
        _ => return Err(Error::InvalidField("AlterTableMoveAllStmt.objtype")),
    });
    p.text(" ALL IN TABLESPACE ");
    p.text(named(
        &s.orig_tablespacename,
        "AlterTableMoveAllStmt.orig_tablespacename",
    )?);
    if !s.roles.is_empty() {
        p.text(" OWNED BY ");
        for (i, n) in s.roles.iter().enumerate() {
            let Node::RoleSpec(r) = n else {
                return Err(Error::InvalidField("AlterTableMoveAllStmt.roles"));
            };
            if i > 0 {
                p.text(", ")
            }
            p.text(role(r)?);
        }
    }
    p.text(" SET TABLESPACE ");
    p.text(named(
        &s.new_tablespacename,
        "AlterTableMoveAllStmt.new_tablespacename",
    )?);
    if s.nowait {
        p.text(" NOWAIT")
    }
    Ok(())
}

fn tablespace<'a>(n: &'a Node, p: &mut Plan<'a>) -> Result<bool> {
    match n {
        Node::CreateTableSpaceStmt(s) => {
            p.text("CREATE TABLESPACE ");
            p.text(named(
                &s.tablespacename,
                "CreateTableSpaceStmt.tablespacename",
            )?);
            if let Some(r) = &s.owner {
                p.text(" OWNER ");
                p.text(role(r)?)
            }
            p.text(" LOCATION ");
            p.text(literal(s.location.as_deref().unwrap_or(""))?);
            if !s.options.is_empty() {
                p.text(" WITH ");
                index::reloptions(&s.options, p)?;
            }
        }
        Node::DropTableSpaceStmt(s) => {
            p.text("DROP TABLESPACE ");
            if s.missing_ok {
                p.text("IF EXISTS ")
            }
            p.text(named(
                &s.tablespacename,
                "DropTableSpaceStmt.tablespacename",
            )?);
        }
        Node::AlterTableSpaceOptionsStmt(s) => {
            p.text("ALTER TABLESPACE ");
            p.text(named(
                &s.tablespacename,
                "AlterTableSpaceOptionsStmt.tablespacename",
            )?);
            p.text(if s.is_reset { " RESET " } else { " SET " });
            index::reloptions(&s.options, p)?;
        }
        Node::AlterTableMoveAllStmt(s) => move_all(s, p)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn vacuum(s: &VacuumStmt, p: &mut Plan<'_>) -> Result<()> {
    let mut head = if s.is_vacuumcmd { "VACUUM" } else { "ANALYZE" }.to_owned();
    utility_options(&mut head, &s.options)?;
    p.text(head);
    for (i, n) in s.rels.iter().enumerate() {
        let Node::VacuumRelation(r) = n else {
            return Err(Error::InvalidField("VacuumStmt.rels"));
        };
        p.text(if i == 0 { " " } else { ", " });
        relation(required(&r.relation, "VacuumRelation.relation")?, p, false)?;
        if !r.va_cols.is_empty() {
            p.text("(");
            p.text(names(&r.va_cols, ", ")?);
            p.text(")");
        }
    }
    Ok(())
}

fn reindex(s: &ReindexStmt, p: &mut Plan<'_>) -> Result<()> {
    use ReindexObjectType as T;
    let mut head = String::from("REINDEX");
    utility_options(&mut head, &s.params)?;
    p.text(head);
    p.text(match s.kind {
        T::REINDEX_OBJECT_INDEX => " INDEX",
        T::REINDEX_OBJECT_TABLE => " TABLE",
        T::REINDEX_OBJECT_SCHEMA => " SCHEMA",
        T::REINDEX_OBJECT_SYSTEM => " SYSTEM",
        T::REINDEX_OBJECT_DATABASE => " DATABASE",
        _ => return Err(Error::InvalidField("ReindexStmt.kind")),
    });
    match s.kind {
        T::REINDEX_OBJECT_INDEX | T::REINDEX_OBJECT_TABLE => {
            p.text(" ");
            relation(required(&s.relation, "ReindexStmt.relation")?, p, false)?;
        }
        T::REINDEX_OBJECT_SCHEMA => {
            p.text(" ");
            p.text(named(&s.name, "ReindexStmt.name")?)
        }
        _ => {
            if let Some(name) = &s.name {
                p.text(" ");
                p.text(ident(name)?)
            }
        }
    }
    Ok(())
}

fn create_stats<'a>(s: &'a CreateStatsStmt, p: &mut Plan<'a>) -> Result<()> {
    p.text("CREATE STATISTICS ");
    if s.if_not_exists {
        p.text("IF NOT EXISTS ");
    }
    if !s.defnames.is_empty() {
        p.text(names(&s.defnames, ".")?);
    }
    p.text(" ");
    if !s.stat_types.is_empty() {
        p.text("(");
        p.text(names(&s.stat_types, ", ")?);
        p.text(") ");
    }
    p.text("ON ");
    if s.exprs.is_empty() || s.relations.is_empty() {
        return Err(Error::InvalidField("CreateStatsStmt.exprs or relations"));
    }
    for (i, node) in s.exprs.iter().enumerate() {
        let Node::StatsElem(e) = node else {
            return Err(Error::InvalidField("CreateStatsStmt.exprs"));
        };
        if i > 0 {
            p.text(", ");
        }
        match (&e.name, e.expr.as_deref()) {
            (Some(name), None) => p.text(ident(name)?),
            (None, Some(expr)) => {
                p.text("(");
                p.expr(expr);
                p.text(")");
            }
            _ => return Err(Error::InvalidField("StatsElem.name or expr")),
        }
    }
    p.text(" FROM ");
    p.list(&s.relations, Role::From);
    Ok(())
}

fn alter_stats(s: &AlterStatsStmt, p: &mut Plan<'_>) -> Result<()> {
    p.text("ALTER STATISTICS ");
    if s.missing_ok {
        p.text("IF EXISTS ");
    }
    if s.defnames.is_empty() {
        return Err(Error::InvalidField("AlterStatsStmt.defnames"));
    }
    p.text(names(&s.defnames, ".")?);
    p.text(" SET STATISTICS ");
    match s.stxstattarget.as_deref() {
        None => p.text("DEFAULT"),
        Some(Node::Integer(i)) => p.text(i.to_string()),
        _ => return Err(Error::InvalidField("AlterStatsStmt.stxstattarget")),
    }
    Ok(())
}

pub(super) fn statement<'a>(n: &'a Node, p: &mut Plan<'a>) -> Result<bool> {
    match n {
        Node::VacuumStmt(s) => vacuum(s, p)?,
        Node::CreateStatsStmt(s) => create_stats(s, p)?,
        Node::AlterStatsStmt(s) => alter_stats(s, p)?,
        Node::ReindexStmt(s) => reindex(s, p)?,
        _ => return Ok(database(n, p)? || tablespace(n, p)?),
    }
    Ok(true)
}
