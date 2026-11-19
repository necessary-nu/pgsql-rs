// INSERT, UPDATE, DELETE and MERGE from libpg_query's postgres_deparse.c.
// PG19 also permits ON CONFLICT DO SELECT with optional row locking.
use super::expr::indirection;
use super::operator::{precedence, Prec};
use super::query::{named, relation, with};
use super::writer::{names, required, Plan, Role};
use super::{ident, Error, Result};
use pg19_nodes::*;
use std::sync::Arc;

fn where_clause<'a>(w: Option<&'a Node>, p: &mut Plan<'a>) -> Result<()> {
    if let Some(n) = w {
        p.clause("WHERE ");
        if let Node::CurrentOfExpr(c) = n {
            p.text("CURRENT OF ");
            p.text(named(&c.cursor_name, "CurrentOfExpr.cursor_name")?);
        } else {
            p.expr(n);
        }
    }
    Ok(())
}
fn returning<'a>(r: Option<&'a ReturningClause>, p: &mut Plan<'a>) -> Result<()> {
    let Some(r) = r else {
        return Ok(());
    };
    p.clause("RETURNING ");
    if !r.options.is_empty() {
        p.text("WITH (");
        for (i, n) in r.options.iter().enumerate() {
            let Node::ReturningOption(o) = n else {
                return Err(Error::InvalidField("ReturningClause.options"));
            };
            if i > 0 {
                p.text(", ");
            }
            p.text(match o.option {
                ReturningOptionKind::RETURNING_OPTION_OLD => "OLD AS ",
                ReturningOptionKind::RETURNING_OPTION_NEW => "NEW AS ",
                _ => return Err(Error::InvalidField("ReturningOption.option")),
            });
            p.text(named(&o.value, "ReturningOption.value")?);
        }
        p.text(") ");
    }
    p.list(&r.exprs, Role::Target);
    Ok(())
}
fn column<'a>(target: &'a ResTarget, p: &mut Plan<'a>) -> Result<()> {
    p.text(named(&target.name, "ResTarget.name")?);
    indirection(&target.indirection, p)
}
fn target(n: &Node) -> Result<&ResTarget> {
    match n {
        Node::ResTarget(t) => Ok(t),
        _ => Err(Error::InvalidField("target list")),
    }
}
fn columns<'a>(cols: &'a [Node], p: &mut Plan<'a>) -> Result<()> {
    if !cols.is_empty() {
        p.text(" (");
        for (i, n) in cols.iter().enumerate() {
            if i > 0 {
                p.text(", ");
            }
            column(target(n)?, p)?;
        }
        p.text(")");
    }
    Ok(())
}
fn overrides(o: OverridingKind, p: &mut Plan<'_>) -> Result<()> {
    match o {
        OverridingKind::OVERRIDING_NOT_SET => {}
        OverridingKind::OVERRIDING_USER_VALUE => p.text(" OVERRIDING USER VALUE"),
        OverridingKind::OVERRIDING_SYSTEM_VALUE => p.text(" OVERRIDING SYSTEM VALUE"),
        _ => return Err(Error::InvalidField("override")),
    }
    Ok(())
}
fn shared_source(a: &Arc<Node>, b: &Arc<Node>) -> Result<bool> {
    if Arc::ptr_eq(a, b) {
        return Ok(true);
    }
    // Protobuf has tree edges only. A decoded multi-assignment holds separate copies.
    let encode =
        |n| pg19_codec::node_json(n).map_err(|_| Error::InvalidField("MultiAssignRef.source"));
    Ok(encode(a)? == encode(b)?)
}
fn assignments<'a>(nodes: &'a [Node], p: &mut Plan<'a>) -> Result<()> {
    let mut i = 0;
    while i < nodes.len() {
        if i != 0 {
            p.comma();
        }
        let t = target(&nodes[i])?;
        let val = required(&t.val, "ResTarget.val")?;
        if let Node::MultiAssignRef(m) = val {
            let count = usize::try_from(m.ncolumns)
                .ok()
                .filter(|&c| c > 0 && c <= nodes.len() - i)
                .ok_or(Error::InvalidField("MultiAssignRef.ncolumns"))?;
            let source = m
                .source
                .as_ref()
                .ok_or(Error::InvalidField("MultiAssignRef.source"))?;
            p.text("(");
            for (j, col) in nodes[i..i + count].iter().enumerate() {
                let col = target(col)?;
                let Node::MultiAssignRef(sibling) = required(&col.val, "ResTarget.val")? else {
                    return Err(Error::InvalidField("multi-column target"));
                };
                if sibling.ncolumns != m.ncolumns
                    || sibling.colno != j as i32 + 1
                    || !shared_source(
                        source,
                        sibling
                            .source
                            .as_ref()
                            .ok_or(Error::InvalidField("MultiAssignRef.source"))?,
                    )?
                {
                    return Err(Error::InvalidField("multi-column target"));
                }
                if j > 0 {
                    p.text(", ");
                }
                column(col, p)?;
            }
            p.text(") = ");
            p.expr(source);
            i += count;
        } else {
            column(t, p)?;
            p.text(" = ");
            p.expr(val);
            i += 1;
        }
    }
    Ok(())
}
pub(super) fn index_elem<'a>(i: &'a IndexElem, p: &mut Plan<'a>) -> Result<()> {
    if let Some(name) = &i.name {
        p.text(ident(name)?);
    } else {
        let expr = required(&i.expr, "IndexElem.expr")?;
        let call = matches!(
            expr,
            Node::FuncCall(_)
                | Node::SQLValueFunction(_)
                | Node::CoalesceExpr(_)
                | Node::MinMaxExpr(_)
                | Node::XmlExpr(_)
                | Node::XmlSerialize(_)
        );
        p.wrapped(expr, Role::Expr, !call || precedence(expr) != Prec::Atom);
    }
    if !i.collation.is_empty() {
        p.text(" COLLATE ");
        p.text(names(&i.collation, ".")?);
    }
    if !i.opclass.is_empty() {
        p.text(" ");
        p.text(names(&i.opclass, ".")?);
    }
    if !i.opclassopts.is_empty() {
        return Err(Error::UnsupportedStatement(Some(NodeTag::IndexElem)));
    }
    p.text(match i.ordering {
        SortByDir::SORTBY_DEFAULT => "",
        SortByDir::SORTBY_ASC => " ASC",
        SortByDir::SORTBY_DESC => " DESC",
        _ => return Err(Error::InvalidField("IndexElem.ordering")),
    });
    p.text(match i.nulls_ordering {
        SortByNulls::SORTBY_NULLS_DEFAULT => "",
        SortByNulls::SORTBY_NULLS_FIRST => " NULLS FIRST",
        SortByNulls::SORTBY_NULLS_LAST => " NULLS LAST",
        _ => return Err(Error::InvalidField("IndexElem.nulls_ordering")),
    });
    Ok(())
}
pub(super) fn lock_strength(l: LockClauseStrength) -> Result<&'static str> {
    match l {
        LockClauseStrength::LCS_FORUPDATE => Ok("FOR UPDATE"),
        LockClauseStrength::LCS_FORNOKEYUPDATE => Ok("FOR NO KEY UPDATE"),
        LockClauseStrength::LCS_FORSHARE => Ok("FOR SHARE"),
        LockClauseStrength::LCS_FORKEYSHARE => Ok("FOR KEY SHARE"),
        _ => Err(Error::InvalidField("locking strength")),
    }
}
fn conflict<'a>(c: &'a OnConflictClause, p: &mut Plan<'a>) -> Result<()> {
    p.clause("ON CONFLICT");
    if let Some(i) = &c.infer {
        if !i.index_elems.is_empty() {
            p.text(" (");
            for (j, n) in i.index_elems.iter().enumerate() {
                let Node::IndexElem(e) = n else {
                    return Err(Error::InvalidField("InferClause.indexElems"));
                };
                if j > 0 {
                    p.text(", ");
                }
                index_elem(e, p)?;
            }
            p.text(")");
        }
        if let Some(constraint) = &i.conname {
            p.text(" ON CONSTRAINT ");
            p.text(ident(constraint)?);
        }
        where_clause(i.where_clause.as_deref(), p)?;
    }
    match c.action {
        OnConflictAction::ONCONFLICT_NOTHING => p.text(" DO NOTHING"),
        OnConflictAction::ONCONFLICT_UPDATE => {
            p.text(" DO UPDATE");
            p.clause("SET ");
            assignments(&c.target_list, p)?;
        }
        OnConflictAction::ONCONFLICT_SELECT => {
            p.text(" DO SELECT");
            if c.lock_strength != LockClauseStrength::LCS_NONE {
                p.text(" ");
                p.text(lock_strength(c.lock_strength)?);
            }
        }
        _ => return Err(Error::InvalidField("OnConflictClause.action")),
    }
    where_clause(c.where_clause.as_deref(), p)
}
pub(super) fn insert<'a>(s: &'a InsertStmt, p: &mut Plan<'a>) -> Result<()> {
    if let Some(w) = &s.with_clause {
        with(w, p);
    }
    p.clause("INSERT INTO ");
    relation(required(&s.relation, "InsertStmt.relation")?, p, true)?;
    columns(&s.cols, p)?;
    overrides(s.r#override, p)?;
    if let Some(n) = s.select_stmt.as_deref() {
        let Node::SelectStmt(q) = n else {
            return Err(Error::InvalidField("InsertStmt.selectStmt"));
        };
        p.select(q, false);
    } else {
        p.text(" DEFAULT VALUES");
    }
    if let Some(c) = &s.on_conflict_clause {
        conflict(c, p)?;
    }
    returning(s.returning_clause.as_deref(), p)
}
pub(super) fn update<'a>(s: &'a UpdateStmt, p: &mut Plan<'a>) -> Result<()> {
    if let Some(w) = &s.with_clause {
        with(w, p);
    }
    p.clause("UPDATE ");
    relation(required(&s.relation, "UpdateStmt.relation")?, p, false)?;
    p.clause("SET ");
    assignments(&s.target_list, p)?;
    if !s.from_clause.is_empty() {
        p.clause("FROM ");
        p.list(&s.from_clause, Role::From);
    }
    where_clause(s.where_clause.as_deref(), p)?;
    returning(s.returning_clause.as_deref(), p)
}
pub(super) fn delete<'a>(s: &'a DeleteStmt, p: &mut Plan<'a>) -> Result<()> {
    if let Some(w) = &s.with_clause {
        with(w, p);
    }
    p.clause("DELETE FROM ");
    relation(required(&s.relation, "DeleteStmt.relation")?, p, false)?;
    if !s.using_clause.is_empty() {
        p.clause("USING ");
        p.list(&s.using_clause, Role::From);
    }
    where_clause(s.where_clause.as_deref(), p)?;
    returning(s.returning_clause.as_deref(), p)
}
fn when<'a>(w: &'a MergeWhenClause, p: &mut Plan<'a>) -> Result<()> {
    p.text(" ");
    p.text(match w.match_kind {
        MergeMatchKind::MERGE_WHEN_MATCHED => "WHEN MATCHED",
        MergeMatchKind::MERGE_WHEN_NOT_MATCHED_BY_SOURCE => "WHEN NOT MATCHED BY SOURCE",
        MergeMatchKind::MERGE_WHEN_NOT_MATCHED_BY_TARGET => "WHEN NOT MATCHED",
        _ => return Err(Error::InvalidField("MergeWhenClause.matchKind")),
    });
    if let Some(c) = &w.condition {
        p.text(" AND ");
        p.expr(c);
    }
    p.text(" THEN ");
    match w.command_type {
        CmdType::CMD_INSERT => {
            p.text("INSERT");
            columns(&w.target_list, p)?;
            overrides(w.r#override, p)?;
            if w.values.is_empty() {
                p.text(" DEFAULT VALUES");
            } else {
                p.text(" VALUES (");
                p.list(&w.values, Role::Expr);
                p.text(")");
            }
        }
        CmdType::CMD_UPDATE => {
            p.text("UPDATE SET ");
            assignments(&w.target_list, p)?;
        }
        CmdType::CMD_DELETE => p.text("DELETE"),
        CmdType::CMD_NOTHING => p.text("DO NOTHING"),
        _ => return Err(Error::InvalidField("MergeWhenClause.commandType")),
    }
    Ok(())
}
pub(super) fn merge<'a>(s: &'a MergeStmt, p: &mut Plan<'a>) -> Result<()> {
    if let Some(w) = &s.with_clause {
        with(w, p);
    }
    p.clause("MERGE INTO ");
    relation(required(&s.relation, "MergeStmt.relation")?, p, false)?;
    p.clause("USING ");
    p.node(
        required(&s.source_relation, "MergeStmt.sourceRelation")?,
        Role::From,
    );
    p.text(" ON ");
    p.expr(required(&s.join_condition, "MergeStmt.joinCondition")?);
    for n in &s.merge_when_clauses {
        let Node::MergeWhenClause(w) = n else {
            return Err(Error::InvalidField("MergeStmt.mergeWhenClauses"));
        };
        when(w, p)?;
    }
    returning(s.returning_clause.as_deref(), p)
}
