mod access;
mod copy;
mod foreign;
mod object;
mod policy;
mod session;
use super::query::{named, relation};
use super::writer::{names, required, Plan, Role};
use super::{ident, literal, Error, Result};
use pg19_nodes::*;

fn role(r: &RoleSpec) -> Result<String> {
    let name = match r.roletype {
        RoleSpecType::ROLESPEC_CSTRING => return named(&r.rolename, "RoleSpec.rolename"),
        RoleSpecType::ROLESPEC_CURRENT_ROLE => "CURRENT_ROLE",
        RoleSpecType::ROLESPEC_CURRENT_USER => "CURRENT_USER",
        RoleSpecType::ROLESPEC_SESSION_USER => "SESSION_USER",
        RoleSpecType::ROLESPEC_PUBLIC => "PUBLIC",
        _ => return Err(Error::InvalidField("RoleSpec.roletype")),
    };
    Ok(name.to_owned())
}

fn relation_list(nodes: &[Node], p: &mut Plan<'_>) -> Result<()> {
    if nodes.is_empty() {
        return Err(Error::InvalidField("relation list"));
    }
    for (i, node) in nodes.iter().enumerate() {
        let Node::RangeVar(r) = node else {
            return Err(Error::InvalidField("relation list"));
        };
        if i > 0 {
            p.text(", ")
        }
        relation(r, p, false)?;
    }
    Ok(())
}

fn behavior(b: DropBehavior, p: &mut Plan<'_>) -> Result<()> {
    match b {
        DropBehavior::DROP_RESTRICT => {}
        DropBehavior::DROP_CASCADE => p.text(" CASCADE"),
        _ => return Err(Error::InvalidField("drop behavior")),
    }
    Ok(())
}

fn enum_change(s: &AlterEnumStmt, p: &mut Plan<'_>) -> Result<()> {
    p.text("ALTER TYPE ");
    p.text(names(&s.type_name, ".")?);
    if let Some(old) = &s.old_val {
        p.text(" RENAME VALUE ");
        p.text(literal(old)?);
        p.text(" TO ");
    } else {
        p.text(" ADD VALUE ");
        if s.skip_if_new_val_exists {
            p.text("IF NOT EXISTS ")
        }
    }
    p.text(literal(
        s.new_val
            .as_deref()
            .ok_or(Error::InvalidField("AlterEnumStmt.newVal"))?,
    )?);
    if let Some(neighbor) = &s.new_val_neighbor {
        p.text(if s.new_val_is_after {
            " AFTER "
        } else {
            " BEFORE "
        });
        p.text(literal(neighbor)?);
    }
    Ok(())
}

pub(super) fn statement<'a>(n: &'a Node, p: &mut Plan<'a>) -> Result<bool> {
    Ok(general(n, p)?
        || access::statement(n, p)?
        || policy::statement(n, p)?
        || foreign::statement(n, p)?)
}

fn general<'a>(n: &'a Node, p: &mut Plan<'a>) -> Result<bool> {
    match n {
        Node::CopyStmt(s) => copy::statement(s, p)?,
        Node::DropStmt(s) => object::drop_objects(s, p)?,
        Node::CommentStmt(s) => {
            p.text("COMMENT ON ");
            object::label(s.objtype, &s.object, &s.comment, p)?;
        }
        Node::SecLabelStmt(s) => {
            p.text("SECURITY LABEL");
            if let Some(provider) = &s.provider {
                p.text(" FOR ");
                p.text(ident(provider)?)
            }
            p.text(" ON ");
            object::label(s.objtype, &s.object, &s.label, p)?;
        }
        Node::TransactionStmt(s) => session::transaction(s, p)?,
        Node::VariableSetStmt(s) => session::setting(s, p)?,
        Node::VariableShowStmt(s) => {
            p.text("SHOW ");
            let name = s
                .name
                .as_deref()
                .ok_or(Error::InvalidField("VariableShowStmt.name"))?;
            p.text(match name {
                "timezone" => "TIME ZONE".to_owned(),
                "transaction_isolation" => "TRANSACTION ISOLATION LEVEL".to_owned(),
                "session_authorization" => "SESSION AUTHORIZATION".to_owned(),
                "all" => "ALL".to_owned(),
                _ => ident(name)?,
            });
        }
        Node::CreateSchemaStmt(s) => {
            p.text("CREATE SCHEMA");
            if s.if_not_exists {
                p.text(" IF NOT EXISTS")
            }
            if let Some(name) = &s.schemaname {
                p.text(" ");
                p.text(ident(name)?)
            }
            if let Some(r) = &s.authrole {
                p.text(" AUTHORIZATION ");
                p.text(role(r)?)
            }
            if s.authrole.is_none() && s.schemaname.is_none() {
                return Err(Error::InvalidField("CreateSchemaStmt.schemaname"));
            }
            for stmt in &s.schema_elts {
                p.text(" ");
                p.statement(stmt);
            }
        }
        Node::CreateEnumStmt(s) => {
            p.text("CREATE TYPE ");
            p.text(names(&s.type_name, ".")?);
            p.text(" AS ENUM (");
            for (i, value) in s.vals.iter().enumerate() {
                let Node::String(s) = value else {
                    return Err(Error::InvalidField("CreateEnumStmt.vals"));
                };
                if i > 0 {
                    p.text(", ")
                }
                p.text(literal(s)?);
            }
            p.text(")");
        }
        Node::AlterEnumStmt(s) => enum_change(s, p)?,
        Node::TruncateStmt(s) => {
            p.text("TRUNCATE ");
            relation_list(&s.relations, p)?;
            if s.restart_seqs {
                p.text(" RESTART IDENTITY")
            }
            behavior(s.behavior, p)?;
        }
        Node::LockStmt(s) => {
            p.text("LOCK TABLE ");
            relation_list(&s.relations, p)?;
            p.text(match s.mode {
                1 => " IN ACCESS SHARE MODE",
                2 => " IN ROW SHARE MODE",
                3 => " IN ROW EXCLUSIVE MODE",
                4 => " IN SHARE UPDATE EXCLUSIVE MODE",
                5 => " IN SHARE MODE",
                6 => " IN SHARE ROW EXCLUSIVE MODE",
                7 => " IN EXCLUSIVE MODE",
                8 => " IN ACCESS EXCLUSIVE MODE",
                _ => return Err(Error::InvalidField("LockStmt.mode")),
            });
            if s.nowait {
                p.text(" NOWAIT")
            }
        }
        Node::RefreshMatViewStmt(s) => {
            p.text("REFRESH MATERIALIZED VIEW ");
            if s.concurrent {
                p.text("CONCURRENTLY ")
            }
            relation(
                required(&s.relation, "RefreshMatViewStmt.relation")?,
                p,
                false,
            )?;
            if s.skip_data {
                p.text(" WITH NO DATA")
            }
        }
        Node::PrepareStmt(s) => {
            p.text("PREPARE ");
            p.text(named(&s.name, "PrepareStmt.name")?);
            if !s.argtypes.is_empty() {
                p.text("(");
                for (i, v) in s.argtypes.iter().enumerate() {
                    if i > 0 {
                        p.text(", ")
                    }
                    let Node::TypeName(t) = v else {
                        return Err(Error::InvalidField("PrepareStmt.argtypes"));
                    };
                    p.typename(t);
                }
                p.text(")")
            }
            p.text(" AS ");
            p.statement(required(&s.query, "PrepareStmt.query")?);
        }
        Node::ExecuteStmt(s) => {
            p.text("EXECUTE ");
            p.text(named(&s.name, "ExecuteStmt.name")?);
            if !s.params.is_empty() {
                p.text("(");
                p.list(&s.params, Role::Expr);
                p.text(")")
            }
        }
        Node::ExplainStmt(s) => {
            p.text("EXPLAIN");
            let mut options = String::new();
            super::utility_options(&mut options, &s.options)?;
            p.text(options);
            p.text(" ");
            p.statement(required(&s.query, "ExplainStmt.query")?);
        }
        Node::ReturnStmt(s) => {
            p.text("RETURN ");
            p.expr(required(&s.returnval, "ReturnStmt.returnval")?)
        }
        _ => return Ok(false),
    }
    Ok(true)
}
