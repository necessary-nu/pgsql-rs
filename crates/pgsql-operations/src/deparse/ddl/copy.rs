use super::{ident, literal, names, relation, Error, Plan, Result};
use crate::deparse::option_value;
use pgsql_nodes::*;

fn option(node: &Node) -> Result<(&str, &DefElem)> {
    let Node::DefElem(d) = node else {
        return Err(Error::InvalidField("CopyStmt.options"));
    };
    Ok((
        d.defname
            .as_deref()
            .ok_or(Error::InvalidField("DefElem.defname"))?,
        d,
    ))
}

fn columns(arg: &Node) -> Result<String> {
    match arg {
        Node::AStar(_) => Ok("*".into()),
        Node::List(cols) if !cols.is_empty() => names(cols, ", "),
        _ => Err(Error::InvalidField("COPY column list")),
    }
}

fn old_option(name: &str, arg: Option<&Node>) -> Result<String> {
    match (name, arg) {
        ("freeze" | "header", Some(Node::Boolean(true))) => Ok(name.to_ascii_uppercase()),
        ("format", Some(Node::String(s))) if matches!(s.as_str(), "csv" | "binary" | "json") => {
            Ok(s.to_ascii_uppercase())
        }
        ("delimiter" | "null" | "quote" | "escape" | "encoding", Some(Node::String(s))) => {
            Ok(format!("{} {}", name.to_ascii_uppercase(), literal(s)?))
        }
        ("force_quote" | "force_not_null" | "force_null", Some(arg)) => Ok(format!(
            "{} {}",
            name.replace('_', " ").to_ascii_uppercase(),
            columns(arg)?
        )),
        _ => Err(Error::InvalidField("legacy COPY option")),
    }
}

fn modern_arg(name: &str, arg: &Node) -> Result<String> {
    match (name, arg) {
        ("format", Node::String(s)) if matches!(s.as_str(), "binary" | "csv" | "text" | "json") => {
            Ok(s.to_ascii_uppercase())
        }
        ("delimiter" | "null" | "quote" | "escape" | "encoding", Node::String(s)) => literal(s),
        ("header" | "freeze", Node::String(s))
            if matches!(s.as_str(), "true" | "false" | "on" | "off" | "match") =>
        {
            Ok(s.to_owned())
        }
        ("force_quote" | "force_null" | "force_not_null", Node::List(cols)) if !cols.is_empty() => {
            Ok(format!("({})", names(cols, ", ")?))
        }
        (_, Node::AStar(_)) => Ok("*".to_owned()),
        (_, Node::List(items)) if !items.is_empty() => {
            let values = items.iter().map(option_value).collect::<Result<Vec<_>>>()?;
            Ok(format!("({})", values.join(", ")))
        }
        (_, arg) => option_value(arg),
    }
}

fn options(nodes: &[Node], p: &mut Plan<'_>) -> Result<()> {
    if nodes.is_empty() {
        return Ok(());
    }
    let items = nodes.iter().map(option).collect::<Result<Vec<_>>>()?;
    // Only the old grammar produces Boolean nodes. Preserve their node type even
    // when other options on the same command have a parenthesized spelling.
    let legacy = items
        .iter()
        .any(|(_, d)| matches!(d.arg.as_deref(), Some(Node::Boolean(_))))
        || items.iter().all(|(name, d)| {
            matches!((*name,d.arg.as_deref()),
            ("format",Some(Node::String(s))) if s == "csv")
                || matches!(
                    (*name, d.arg.as_deref()),
                    ("force_quote", Some(Node::List(_)))
                )
        });
    if !legacy {
        p.text(" WITH (");
    }
    for (i, (name, d)) in items.into_iter().enumerate() {
        p.mark(d.location);
        if legacy {
            p.text(" ");
            p.text(old_option(name, d.arg.as_deref())?);
        } else {
            if i > 0 {
                p.text(", ");
            }
            p.text(match name {
                "format" | "header" | "freeze" | "delimiter" | "null" | "quote" | "escape"
                | "encoding" | "force_quote" | "force_not_null" | "force_null" => {
                    name.to_ascii_uppercase()
                }
                _ => ident(name)?,
            });
            if let Some(arg) = d.arg.as_deref() {
                p.text(" ");
                p.text(modern_arg(name, arg)?);
            }
        }
    }
    if !legacy {
        p.text(")");
    }
    Ok(())
}

pub(super) fn statement<'a>(s: &'a CopyStmt, p: &mut Plan<'a>) -> Result<()> {
    p.text("COPY ");
    match (&s.relation, &s.query) {
        (Some(table), None) => {
            relation(table, p, false)?;
            if !s.attlist.is_empty() {
                p.text("(");
                p.text(names(&s.attlist, ", ")?);
                p.text(")");
            }
        }
        (None, Some(query)) if s.attlist.is_empty() => {
            p.text("(");
            p.statement(query);
            p.text(")");
        }
        _ => return Err(Error::InvalidField("CopyStmt.relation or query")),
    }
    p.text(if s.is_from { " FROM " } else { " TO " });
    if s.is_program {
        p.text("PROGRAM ");
    }
    match &s.filename {
        Some(path) => p.text(literal(path)?),
        None if s.is_program => return Err(Error::InvalidField("CopyStmt.filename")),
        None => p.text(if s.is_from { "STDIN" } else { "STDOUT" }),
    }
    options(&s.options, p)?;
    if let Some(filter) = &s.where_clause {
        p.clause("WHERE ");
        p.expr(filter);
    }
    Ok(())
}
