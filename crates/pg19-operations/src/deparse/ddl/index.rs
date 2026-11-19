use crate::deparse::dml::index_elem;
use crate::deparse::query::relation;
use crate::deparse::writer::{required, Plan};
use crate::deparse::{ident, literal, numeric, Error, Result};
use pg19_nodes::*;

fn option_value<'a>(n: &'a Node, p: &mut Plan<'a>) -> Result<()> {
    match n {
        Node::TypeName(t) => p.typename(t),
        Node::Integer(i) => p.text(i.to_string()),
        Node::Float(s) => p.text(numeric(s)?),
        Node::String(s) if s == "none" => p.text("NONE"),
        Node::String(s)
            if pg19_lexer::keyword(s)
                .is_some_and(|(_, _, k)| k == pg19_lexer::KeywordKind::Reserved)
                && s.bytes().all(|b| !b.is_ascii_uppercase()) =>
        {
            p.text(s.as_str())
        }
        Node::String(s) => p.text(literal(s)?),
        Node::List(parts) => p.text(crate::deparse::operator::operator(parts)?),
        _ => return Err(Error::InvalidField("reloption value")),
    }
    Ok(())
}

pub(in crate::deparse) fn reloptions<'a>(options: &'a [Node], p: &mut Plan<'a>) -> Result<()> {
    p.text("(");
    for (i, n) in options.iter().enumerate() {
        if i > 0 {
            p.text(", ");
        }
        let Node::DefElem(d) = n else {
            return Err(Error::InvalidField("reloptions"));
        };
        if let Some(s) = &d.defnamespace {
            p.text(ident(s)?);
            p.text(".");
        }
        if let Some(s) = &d.defname {
            p.text(ident(s)?);
        }
        if let Some(v) = d.arg.as_deref() {
            if d.defname.is_some() {
                p.text("=");
            }
            option_value(v, p)?;
        }
    }
    p.text(")");
    Ok(())
}

fn columns<'a>(cols: &'a [Node], p: &mut Plan<'a>) -> Result<()> {
    p.text("(");
    for (i, n) in cols.iter().enumerate() {
        if i > 0 {
            p.text(", ");
        }
        let Node::IndexElem(e) = n else {
            return Err(Error::InvalidField("IndexStmt.indexParams"));
        };
        index_elem(e, p)?;
    }
    p.text(")");
    Ok(())
}

pub(super) fn index<'a>(s: &'a IndexStmt, p: &mut Plan<'a>) -> Result<()> {
    p.text(if s.unique {
        "CREATE UNIQUE INDEX "
    } else {
        "CREATE INDEX "
    });
    if s.concurrent {
        p.text("CONCURRENTLY ");
    }
    if s.if_not_exists {
        p.text("IF NOT EXISTS ");
    }
    if let Some(i) = &s.idxname {
        p.text(ident(i)?);
    }
    p.clause("ON ");
    relation(required(&s.relation, "IndexStmt.relation")?, p, false)?;
    if let Some(method) = &s.access_method {
        p.text(" USING ");
        p.text(ident(method)?);
    }
    p.text(" ");
    columns(&s.index_params, p)?;
    if !s.index_including_params.is_empty() {
        p.text(" INCLUDE ");
        columns(&s.index_including_params, p)?;
    }
    if s.nulls_not_distinct {
        p.text(" NULLS NOT DISTINCT");
    }
    if !s.options.is_empty() {
        p.text(" WITH ");
        reloptions(&s.options, p)?;
    }
    if let Some(space) = &s.table_space {
        p.text(" TABLESPACE ");
        p.text(ident(space)?);
    }
    if let Some(w) = s.where_clause.as_deref() {
        p.clause("WHERE ");
        p.expr(w);
    }
    Ok(())
}
