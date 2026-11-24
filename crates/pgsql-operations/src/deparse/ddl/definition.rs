use super::{
    behavior, constraints, ident, named, names, relation, required, table, Error, Plan, Result,
};
use pgsql_nodes::*;

fn domain<'a>(s: &'a CreateDomainStmt, p: &mut Plan<'a>) -> Result<()> {
    p.text("CREATE DOMAIN ");
    p.text(names(&s.domainname, ".")?);
    p.text(" AS ");
    p.typename(required(&s.type_name, "CreateDomainStmt.typeName")?);
    if let Some(c) = &s.coll_clause {
        p.text(" COLLATE ");
        p.text(names(&c.collname, ".")?)
    }
    for n in &s.constraints {
        let Node::Constraint(c) = n else {
            return Err(Error::InvalidField("CreateDomainStmt.constraints"));
        };
        p.text(" ");
        constraints::constraint(c, true, p)?;
    }
    Ok(())
}
fn change_domain<'a>(s: &'a AlterDomainStmt, p: &mut Plan<'a>) -> Result<()> {
    use AlterDomainType as K;
    p.text("ALTER DOMAIN ");
    p.text(names(&s.type_name, ".")?);
    match s.subtype {
        K::AD_SetNotNull => p.text(" SET NOT NULL"),
        K::AD_DropNotNull => p.text(" DROP NOT NULL"),
        K::AD_AlterDefault => {
            if let Some(n) = s.def.as_deref() {
                p.text(" SET DEFAULT ");
                p.expr(n)
            } else {
                p.text(" DROP DEFAULT")
            }
        }
        K::AD_AddConstraint => {
            p.text(" ADD ");
            let Some(Node::Constraint(c)) = s.def.as_deref() else {
                return Err(Error::InvalidField("AlterDomainStmt.def"));
            };
            constraints::constraint(c, true, p)?
        }
        K::AD_DropConstraint => {
            p.text(" DROP CONSTRAINT ");
            if s.missing_ok {
                p.text("IF EXISTS ")
            }
            p.text(named(&s.name, "AlterDomainStmt.name")?);
            behavior(s.behavior, p)?
        }
        K::AD_ValidateConstraint => {
            p.text(" VALIDATE CONSTRAINT ");
            p.text(named(&s.name, "AlterDomainStmt.name")?)
        }
        _ => return Err(Error::InvalidField("AlterDomainStmt.subtype")),
    }
    Ok(())
}
fn composite<'a>(s: &'a CompositeTypeStmt, p: &mut Plan<'a>) -> Result<()> {
    p.text("CREATE TYPE ");
    super::relation_name(required(&s.typevar, "CompositeTypeStmt.typevar")?, p)?;
    p.text(" AS (");
    p.enter();
    for (i, n) in s.coldeflist.iter().enumerate() {
        let Node::ColumnDef(c) = n else {
            return Err(Error::InvalidField("composite attributes"));
        };
        if i > 0 {
            p.comma()
        }
        table::column(c, p)?;
    }
    p.leave();
    p.text(")");
    Ok(())
}

fn view<'a>(s: &'a ViewStmt, p: &mut Plan<'a>) -> Result<()> {
    let r = required(&s.view, "ViewStmt.view")?;
    p.text("CREATE ");
    if s.replace {
        p.text("OR REPLACE ")
    }
    table::persistence(r, p)?;
    p.text("VIEW ");
    relation(r, p, false)?;
    if !s.aliases.is_empty() {
        p.text(" (");
        p.text(names(&s.aliases, ", ")?);
        p.text(")")
    }
    if !s.options.is_empty() {
        p.text(" WITH ");
        super::index::reloptions(&s.options, p)?;
    }
    p.text(" AS ");
    p.statement(required(&s.query, "ViewStmt.query")?);
    match s.with_check_option {
        ViewCheckOption::NO_CHECK_OPTION => {}
        ViewCheckOption::LOCAL_CHECK_OPTION => p.text(" WITH LOCAL CHECK OPTION"),
        ViewCheckOption::CASCADED_CHECK_OPTION => p.text(" WITH CHECK OPTION"),
        _ => return Err(Error::InvalidField("ViewStmt.withCheckOption")),
    }
    Ok(())
}

fn create_as<'a>(s: &'a CreateTableAsStmt, p: &mut Plan<'a>) -> Result<()> {
    let into = required(&s.into, "CreateTableAsStmt.into")?;
    let r = required(&into.rel, "IntoClause.rel")?;
    p.text("CREATE ");
    table::persistence(r, p)?;
    p.text(match s.objtype {
        ObjectType::OBJECT_TABLE => "TABLE ",
        ObjectType::OBJECT_MATVIEW => "MATERIALIZED VIEW ",
        _ => return Err(Error::InvalidField("CreateTableAsStmt.objtype")),
    });
    if s.if_not_exists {
        p.text("IF NOT EXISTS ")
    }
    relation(r, p, false)?;
    if !into.col_names.is_empty() {
        p.text("(");
        p.text(names(&into.col_names, ", ")?);
        p.text(")")
    }
    if let Some(am) = &into.access_method {
        p.text(" USING ");
        p.text(ident(am)?)
    }
    if !into.options.is_empty() {
        p.text(" WITH ");
        super::index::reloptions(&into.options, p)?;
    }
    match into.on_commit {
        OnCommitAction::ONCOMMIT_NOOP => {}
        OnCommitAction::ONCOMMIT_DROP => p.text(" ON COMMIT DROP"),
        OnCommitAction::ONCOMMIT_DELETE_ROWS => p.text(" ON COMMIT DELETE ROWS"),
        OnCommitAction::ONCOMMIT_PRESERVE_ROWS => p.text(" ON COMMIT PRESERVE ROWS"),
        _ => return Err(Error::InvalidField("IntoClause.onCommit")),
    };
    if let Some(ts) = &into.table_space_name {
        p.text(" TABLESPACE ");
        p.text(ident(ts)?)
    }
    p.text(" AS ");
    p.statement(required(&s.query, "CreateTableAsStmt.query")?);
    if into.skip_data {
        p.text("  WITH NO DATA")
    }
    Ok(())
}

pub(super) fn statement<'a>(n: &'a Node, p: &mut Plan<'a>) -> Result<bool> {
    match n {
        Node::CreateDomainStmt(d) => domain(d, p)?,
        Node::AlterDomainStmt(d) => change_domain(d, p)?,
        Node::CompositeTypeStmt(s) => composite(s, p)?,
        Node::ViewStmt(s) => view(s, p)?,
        Node::CreateTableAsStmt(s) => create_as(s, p)?,
        Node::CreateSeqStmt(s) => super::sequence::create(s, p)?,
        Node::AlterSeqStmt(s) => super::sequence::alter(s, p)?,
        _ => return super::ownership::statement(n, p),
    }
    Ok(true)
}
