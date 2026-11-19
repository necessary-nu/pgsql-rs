use super::{access::role_list, behavior, named, relation, required, role, Error, Plan, Result};
use pg19_nodes::*;

fn target(name: &Option<String>, table: &Option<Box<RangeVar>>, p: &mut Plan<'_>) -> Result<()> {
    p.text(named(name, "policy name")?);
    p.text(" ON ");
    relation(required(table, "policy table")?, p, false)
}

fn clauses<'a>(
    roles: &[Node],
    filter: Option<&'a Node>,
    check: Option<&'a Node>,
    p: &mut Plan<'a>,
) -> Result<()> {
    if !roles.is_empty() {
        p.text(" TO ");
        role_list(roles, p)?;
    }
    for (prefix, expr) in [(" USING (", filter), (" WITH CHECK (", check)] {
        if let Some(n) = expr {
            p.text(prefix);
            p.expr(n);
            p.text(")");
        }
    }
    Ok(())
}

pub(super) fn statement<'a>(n: &'a Node, p: &mut Plan<'a>) -> Result<bool> {
    match n {
        Node::CreatePolicyStmt(s) => {
            p.text("CREATE POLICY ");
            target(&s.policy_name, &s.table, p)?;
            if !s.permissive {
                p.text(" AS RESTRICTIVE");
            }
            p.text(match s.cmd_name.as_deref() {
                Some("all") => "",
                Some("select") => " FOR SELECT",
                Some("update") => " FOR UPDATE",
                Some("insert") => " FOR INSERT",
                Some("delete") => " FOR DELETE",
                _ => return Err(Error::InvalidField("CreatePolicyStmt.cmd_name")),
            });
            clauses(&s.roles, s.qual.as_deref(), s.with_check.as_deref(), p)?;
        }
        Node::AlterPolicyStmt(s) => {
            p.text("ALTER POLICY ");
            target(&s.policy_name, &s.table, p)?;
            clauses(&s.roles, s.qual.as_deref(), s.with_check.as_deref(), p)?;
        }
        Node::DropOwnedStmt(s) => {
            p.text("DROP OWNED BY ");
            role_list(&s.roles, p)?;
            behavior(s.behavior, p)?;
        }
        Node::ReassignOwnedStmt(s) => {
            p.text("REASSIGN OWNED BY ");
            role_list(&s.roles, p)?;
            p.text(" TO ");
            p.text(role(required(&s.newrole, "ReassignOwnedStmt.newrole")?)?);
        }
        _ => return Ok(false),
    }
    Ok(true)
}
