// PostgreSQL parsenodes.h packs start and end bounds into adjacent bits.
use super::query::named;
use super::writer::{Plan, Role};
use super::{ident, Error, Result};
use pgsql_nodes::*;
const UNBOUNDED_PRECEDING: i32 = 0x20;
const UNBOUNDED_FOLLOWING: i32 = 0x80;
const CURRENT_ROW: i32 = 0x200;
const OFFSET_PRECEDING: i32 = 0x800;
const OFFSET_FOLLOWING: i32 = 0x2000;
const BOUNDS: i32 =
    UNBOUNDED_PRECEDING | UNBOUNDED_FOLLOWING | CURRENT_ROW | OFFSET_PRECEDING | OFFSET_FOLLOWING;

fn bound<'a>(bits: i32, offset: Option<&'a Node>, p: &mut Plan<'a>) -> Result<()> {
    match (bits & BOUNDS, offset) {
        (UNBOUNDED_PRECEDING, None) => p.text("UNBOUNDED PRECEDING"),
        (UNBOUNDED_FOLLOWING, None) => p.text("UNBOUNDED FOLLOWING"),
        (CURRENT_ROW, None) => p.text("CURRENT ROW"),
        (OFFSET_PRECEDING, Some(expr)) => {
            p.expr(expr);
            p.text(" PRECEDING");
        }
        (OFFSET_FOLLOWING, Some(expr)) => {
            p.expr(expr);
            p.text(" FOLLOWING");
        }
        _ => return Err(Error::InvalidField("WindowDef frame bound")),
    }
    Ok(())
}
fn frame<'a>(w: &'a WindowDef, p: &mut Plan<'a>) -> Result<()> {
    let bits = w.frame_options;
    p.text(match bits & 0xe {
        2 => "RANGE ",
        4 => "ROWS ",
        8 => "GROUPS ",
        _ => return Err(Error::InvalidField("WindowDef frame mode")),
    });
    let between = bits & 0x10 != 0;
    if between {
        p.text("BETWEEN ");
    }
    bound(bits, w.start_offset.as_deref(), p)?;
    if between {
        p.text(" AND ");
        bound(bits >> 1, w.end_offset.as_deref(), p)?;
    }
    p.text(match bits & 0x38000 {
        0 => "",
        0x8000 => " EXCLUDE CURRENT ROW",
        0x10000 => " EXCLUDE GROUP",
        0x20000 => " EXCLUDE TIES",
        _ => return Err(Error::InvalidField("WindowDef frame exclusion")),
    });
    Ok(())
}
fn definition<'a>(w: &'a WindowDef, p: &mut Plan<'a>) -> Result<()> {
    let custom = w.frame_options & 1 != 0;
    if w.refname.is_none() && w.partition_clause.is_empty() && w.order_clause.is_empty() && !custom
    {
        p.text("()");
        return Ok(());
    }
    p.text("(");
    p.enter();
    if let Some(name) = &w.refname {
        p.text(ident(name)?);
    }
    if !w.partition_clause.is_empty() {
        if w.refname.is_some() {
            p.text(" ");
        }
        p.text("PARTITION BY ");
        p.list(&w.partition_clause, Role::Expr);
    }
    if !w.order_clause.is_empty() {
        if w.refname.is_some() || !w.partition_clause.is_empty() {
            p.cut();
        }
        p.text("ORDER BY ");
        for (i, n) in w.order_clause.iter().enumerate() {
            if i > 0 {
                p.text(", ");
            }
            p.node(n, Role::Sort);
        }
    }
    if custom {
        if w.refname.is_some() || !w.partition_clause.is_empty() || !w.order_clause.is_empty() {
            p.cut();
        }
        frame(w, p)?;
    }
    p.leave();
    p.text(")");
    Ok(())
}
pub(super) fn over<'a>(w: &'a WindowDef, p: &mut Plan<'a>) -> Result<()> {
    p.text(" OVER ");
    if w.name.is_some() {
        p.text(named(&w.name, "WindowDef.name")?);
        Ok(())
    } else {
        definition(w, p)
    }
}
pub(super) fn declarations<'a>(defs: &'a [Node], p: &mut Plan<'a>) -> Result<()> {
    if defs.is_empty() {
        return Ok(());
    }
    p.clause("WINDOW ");
    for (i, n) in defs.iter().enumerate() {
        let Node::WindowDef(w) = n else {
            return Err(Error::InvalidField("SelectStmt.windowClause"));
        };
        if i > 0 {
            p.text(", ");
        }
        p.text(named(&w.name, "WindowDef.name")?);
        p.text(" AS ");
        definition(w, p)?;
    }
    Ok(())
}
