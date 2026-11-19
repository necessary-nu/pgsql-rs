// SELECT clauses, set-operation arms and table references from postgres_deparse.c.
use super::operator::{operator, precedence, Prec};
use super::writer::{names, required, Plan, Role};
use super::{ident, Error, Result};
use pg19_nodes::*;

pub(super) fn named(name: &Option<String>, field: &'static str) -> Result<String> {
    let Some(name) = name else {
        return Err(Error::InvalidField(field));
    };
    ident(name)
}
pub(super) fn alias(a: &Alias) -> Result<String> {
    let mut s = named(&a.aliasname, "Alias.aliasname")?;
    if !a.colnames.is_empty() {
        s.push('(');
        s.push_str(&names(&a.colnames, ", ")?);
        s.push(')');
    }
    Ok(s)
}
pub(super) fn relation(r: &RangeVar, p: &mut Plan<'_>, insert: bool) -> Result<()> {
    if !r.inh {
        p.text("ONLY ");
    }
    for part in [&r.catalogname, &r.schemaname].into_iter().flatten() {
        p.text(ident(part)?);
        p.text(".");
    }
    p.text(named(&r.relname, "RangeVar.relname")?);
    if let Some(a) = &r.alias {
        p.text(if insert { " AS " } else { " " });
        p.text(alias(a)?);
    }
    Ok(())
}
fn join<'a>(j: &'a JoinExpr, p: &mut Plan<'a>) -> Result<()> {
    let left = required(&j.larg, "JoinExpr.larg")?;
    let right = required(&j.rarg, "JoinExpr.rarg")?;
    if j.alias.is_some() {
        p.text("(");
    }
    p.node(left, Role::From);
    p.cut();
    if j.is_natural {
        p.text("NATURAL ");
    }
    p.text(match j.jointype {
        JoinType::JOIN_INNER if !j.is_natural && j.quals.is_none() && j.using_clause.is_empty() => {
            "CROSS JOIN "
        }
        JoinType::JOIN_INNER => "JOIN ",
        JoinType::JOIN_LEFT => "LEFT JOIN ",
        JoinType::JOIN_RIGHT => "RIGHT JOIN ",
        JoinType::JOIN_FULL => "FULL JOIN ",
        _ => return Err(Error::InvalidField("JoinExpr.jointype")),
    });
    p.wrapped(
        right,
        Role::From,
        matches!(right,Node::JoinExpr(k) if k.alias.is_none()),
    );
    if let Some(on) = &j.quals {
        p.text(" ON ");
        p.expr(on);
    }
    if !j.using_clause.is_empty() {
        p.text(" USING (");
        p.text(names(&j.using_clause, ", ")?);
        p.text(")");
    }
    if let Some(a) = &j.join_using_alias {
        p.text(" AS ");
        p.text(alias(a)?);
    }
    if let Some(a) = &j.alias {
        p.text(if j.join_using_alias.is_some() {
            ") "
        } else {
            " ) "
        });
        p.text(alias(a)?);
    }
    Ok(())
}
fn from<'a>(n: &'a Node, p: &mut Plan<'a>) -> Result<()> {
    match n {
        Node::RangeVar(r) => return relation(r, p, false),
        Node::JoinExpr(j) => return join(j, p),
        Node::RangeSubselect(s) => {
            if s.lateral {
                p.text("LATERAL ");
            }
            p.text("(");
            p.statement(required(&s.subquery, "RangeSubselect.subquery")?);
            p.text(")");
            if let Some(a) = &s.alias {
                p.text(" ");
                p.text(alias(a)?);
            }
        }
        Node::RangeFunction(f) => {
            if !f.coldeflist.is_empty() {
                return Err(Error::UnsupportedStatement(n.tag()));
            }
            if f.lateral {
                p.text("LATERAL ");
            }
            if f.is_rowsfrom {
                p.text("ROWS FROM (");
            }
            for (i, element) in f.functions.iter().enumerate() {
                let Node::List(pair) = element else {
                    return Err(Error::InvalidField("RangeFunction.functions"));
                };
                let [call, Node::Null] = pair.as_slice() else {
                    return Err(Error::UnsupportedStatement(n.tag()));
                };
                if i > 0 {
                    p.text(", ");
                }
                if let Node::TypeCast(t) = call {
                    p.text("CAST(");
                    p.expr(required(&t.arg, "TypeCast.arg")?);
                    p.text(" AS ");
                    p.typename(required(&t.type_name, "TypeCast.typeName")?);
                    p.text(")");
                } else {
                    p.expr(call);
                }
            }
            if f.is_rowsfrom {
                p.text(")");
            }
            if f.ordinality {
                p.text(" WITH ORDINALITY");
            }
            if let Some(a) = &f.alias {
                p.text(" ");
                p.text(alias(a)?);
            }
        }
        _ => return Err(Error::UnsupportedStatement(n.tag())),
    }
    Ok(())
}
fn sort<'a>(s: &'a SortBy, p: &mut Plan<'a>) -> Result<()> {
    p.expr(required(&s.node, "SortBy.node")?);
    match s.sortby_dir {
        SortByDir::SORTBY_DEFAULT => {}
        SortByDir::SORTBY_ASC => p.text(" ASC"),
        SortByDir::SORTBY_DESC => p.text(" DESC"),
        SortByDir::SORTBY_USING => {
            p.text(" USING ");
            p.text(operator(&s.use_op)?);
        }
        _ => return Err(Error::InvalidField("SortBy.sortby_dir")),
    }
    match s.sortby_nulls {
        SortByNulls::SORTBY_NULLS_DEFAULT => {}
        SortByNulls::SORTBY_NULLS_FIRST => p.text(" NULLS FIRST"),
        SortByNulls::SORTBY_NULLS_LAST => p.text(" NULLS LAST"),
        _ => return Err(Error::InvalidField("SortBy.sortby_nulls")),
    }
    Ok(())
}
fn cte<'a>(c: &'a CommonTableExpr, p: &mut Plan<'a>) -> Result<()> {
    p.mark(c.location);
    p.text(named(&c.ctename, "CommonTableExpr.ctename")?);
    if !c.aliascolnames.is_empty() {
        p.text("(");
        p.text(names(&c.aliascolnames, ", ")?);
        p.text(")");
    }
    p.text(match c.ctematerialized {
        CTEMaterialize::CTEMaterializeDefault => " AS (",
        CTEMaterialize::CTEMaterializeAlways => " AS MATERIALIZED (",
        CTEMaterialize::CTEMaterializeNever => " AS NOT MATERIALIZED (",
        _ => return Err(Error::InvalidField("CommonTableExpr.ctematerialized")),
    });
    p.statement(required(&c.ctequery, "CommonTableExpr.ctequery")?);
    p.text(")");
    if let Some(s) = &c.search_clause {
        p.text(if s.search_breadth_first {
            " SEARCH BREADTH FIRST BY "
        } else {
            " SEARCH DEPTH FIRST BY "
        });
        p.text(names(&s.search_col_list, ", ")?);
        p.text(" SET ");
        p.text(named(
            &s.search_seq_column,
            "CTESearchClause.search_seq_column",
        )?);
    }
    if let Some(s) = &c.cycle_clause {
        p.text(" CYCLE ");
        p.text(names(&s.cycle_col_list, ", ")?);
        p.text(" SET ");
        p.text(named(
            &s.cycle_mark_column,
            "CTECycleClause.cycle_mark_column",
        )?);
        if let Some(v) = &s.cycle_mark_value {
            p.text(" TO ");
            p.expr(v);
        }
        if let Some(v) = &s.cycle_mark_default {
            p.text(" DEFAULT ");
            p.expr(v);
        }
        p.text(" USING ");
        p.text(named(
            &s.cycle_path_column,
            "CTECycleClause.cycle_path_column",
        )?);
    }
    Ok(())
}
pub(super) fn item<'a>(n: &'a Node, role: Role, p: &mut Plan<'a>) -> Result<()> {
    match (role, n) {
        (Role::Target, Node::ResTarget(t)) => {
            if !t.indirection.is_empty() {
                return Err(Error::InvalidField("target indirection"));
            }

            p.expr(required(&t.val, "ResTarget.val")?);
            if let Some(name) = &t.name {
                p.text(" AS ");
                p.text(ident(name)?);
            }
        }
        (Role::Sort, Node::SortBy(s)) => return sort(s, p),
        (Role::From, _) => return from(n, p),
        (Role::Group, Node::GroupingSet(g)) => {
            p.text(match g.kind {
                GroupingSetKind::GROUPING_SET_EMPTY | GroupingSetKind::GROUPING_SET_SIMPLE => "(",
                GroupingSetKind::GROUPING_SET_ROLLUP => "ROLLUP (",
                GroupingSetKind::GROUPING_SET_CUBE => "CUBE (",
                GroupingSetKind::GROUPING_SET_SETS => "GROUPING SETS (",
                _ => return Err(Error::InvalidField("GroupingSet.kind")),
            });
            p.list(&g.content, Role::Group);
            p.text(")");
        }
        (Role::Group, _) => p.expr(n),
        (Role::Cte, Node::CommonTableExpr(c)) => return cte(c, p),
        _ => return Err(Error::UnsupportedStatement(n.tag())),
    }
    Ok(())
}
pub(super) fn with<'a>(w: &'a WithClause, p: &mut Plan<'a>) {
    p.clause(if w.recursive {
        "WITH RECURSIVE "
    } else {
        "WITH "
    });
    p.list(&w.ctes, Role::Cte);
}
fn simple<'a>(s: &'a SelectStmt, p: &mut Plan<'a>) -> Result<()> {
    if !s.values_lists.is_empty() {
        p.clause("VALUES ");
        for (i, row) in s.values_lists.iter().enumerate() {
            let Node::List(values) = row else {
                return Err(Error::InvalidField("SelectStmt.valuesLists"));
            };
            if i > 0 {
                p.comma();
            }
            p.text("(");
            p.list(values, Role::Expr);
            p.text(")");
        }
        return Ok(());
    }
    p.clause("SELECT");
    if !s.distinct_clause.is_empty() {
        p.text(" DISTINCT");
        p.separate();
        if !matches!(s.distinct_clause.as_slice(), [Node::Null]) {
            p.text(" ON (");
            p.list(&s.distinct_clause, Role::Expr);
            p.text(")");
        }
    }
    if !s.target_list.is_empty() {
        if !s.distinct_clause.is_empty() {
            p.cut();
        } else {
            p.text(" ");
        }
        p.list(&s.target_list, Role::Target);
    }
    if s.into_clause.is_some() || !s.window_clause.is_empty() {
        return Err(Error::UnsupportedStatement(Some(NodeTag::SelectStmt)));
    }
    if !s.from_clause.is_empty() {
        p.clause("FROM ");
        p.list(&s.from_clause, Role::From);
    }
    if let Some(w) = &s.where_clause {
        p.clause("WHERE ");
        p.expr(w);
    }
    if !s.group_clause.is_empty() {
        p.clause("GROUP BY ");
        if s.group_distinct {
            p.text("DISTINCT ");
        }
        p.list(&s.group_clause, Role::Group);
    }
    if let Some(h) = &s.having_clause {
        p.clause("HAVING ");
        p.expr(h);
    }
    Ok(())
}
fn select_tail<'a>(s: &'a SelectStmt, p: &mut Plan<'a>) -> Result<()> {
    if !s.sort_clause.is_empty() {
        p.clause("ORDER BY ");
        p.list(&s.sort_clause, Role::Sort);
    }
    if let Some(n) = &s.limit_count {
        let ties = s.limit_option == LimitOption::LIMIT_OPTION_WITH_TIES;
        p.clause(if ties { "FETCH FIRST " } else { "LIMIT " });
        if matches!(
            n.as_ref(),
            Node::AConst(AConst {
                val: Literal::Null,
                ..
            })
        ) {
            p.text("ALL");
        } else {
            p.wrapped(n, Role::Expr, ties && precedence(n) < Prec::Atom);
        }
        if ties {
            p.text(" ROWS WITH TIES");
        }
    }
    if let Some(offset) = &s.limit_offset {
        p.clause("OFFSET ");
        p.expr(offset);
    }
    for lock in &s.locking_clause {
        let Node::LockingClause(l) = lock else {
            return Err(Error::InvalidField("SelectStmt.lockingClause"));
        };
        p.clause(super::dml::lock_strength(l.strength)?);
        if !l.locked_rels.is_empty() {
            p.text(" OF ");
            for (i, r) in l.locked_rels.iter().enumerate() {
                if i > 0 {
                    p.text(", ");
                }
                p.node(r, Role::From);
            }
        }
        match l.wait_policy {
            LockWaitPolicy::LockWaitBlock => {}
            LockWaitPolicy::LockWaitSkip => p.text(" SKIP LOCKED"),
            LockWaitPolicy::LockWaitError => p.text(" NOWAIT"),
            _ => return Err(Error::InvalidField("LockingClause.waitPolicy")),
        }
    }
    Ok(())
}
pub(super) fn select<'a>(s: &'a SelectStmt, arm: bool, p: &mut Plan<'a>) -> Result<()> {
    let bracket = arm
        && (s.op != SetOperation::SETOP_NONE
            || s.with_clause.is_some()
            || !s.sort_clause.is_empty()
            || s.limit_offset.is_some()
            || s.limit_count.is_some()
            || !s.locking_clause.is_empty());
    if bracket {
        p.text("(");
        p.enter();
    }
    if let Some(w) = &s.with_clause {
        with(w, p);
    }
    if s.op == SetOperation::SETOP_NONE {
        simple(s, p)?;
    } else {
        p.select(
            s.larg
                .as_deref()
                .ok_or(Error::InvalidField("SelectStmt.larg"))?,
            true,
        );
        p.clause(match s.op {
            SetOperation::SETOP_UNION => "UNION ",
            SetOperation::SETOP_INTERSECT => "INTERSECT ",
            SetOperation::SETOP_EXCEPT => "EXCEPT ",
            _ => return Err(Error::InvalidField("SelectStmt.op")),
        });
        if s.all {
            p.text("ALL ");
        }
        p.select(
            s.rarg
                .as_deref()
                .ok_or(Error::InvalidField("SelectStmt.rarg"))?,
            true,
        );
    }
    select_tail(s, p)?;
    if bracket {
        p.leave();
        p.text(")");
    }
    Ok(())
}
