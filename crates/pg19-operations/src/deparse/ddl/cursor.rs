use super::{named, names, privileges, relation_list, required, session, Error, Plan, Result};
use pg19_nodes::*;

fn declare<'a>(s: &'a DeclareCursorStmt, p: &mut Plan<'a>) -> Result<()> {
    p.text("DECLARE ");
    p.text(named(&s.portalname, "DeclareCursorStmt.portalname")?);
    for (bit, word) in [
        (0x01, " BINARY"),
        (0x02, " SCROLL"),
        (0x04, " NO SCROLL"),
        (0x08, " INSENSITIVE"),
        (0x10, " ASENSITIVE"),
    ] {
        if s.options & bit != 0 {
            p.text(word);
        }
    }
    p.text(" CURSOR");
    if s.options & 0x20 != 0 {
        p.text(" WITH HOLD");
    }
    p.text(" FOR ");
    p.statement(required(&s.query, "DeclareCursorStmt.query")?);
    Ok(())
}

fn fetch(s: &FetchStmt, p: &mut Plan<'_>) -> Result<()> {
    use FetchDirectionKeywords as K;
    p.text(if s.ismove { "MOVE " } else { "FETCH " });
    let (keyword, counted) = match s.direction_keyword {
        K::FETCH_KEYWORD_NONE => ("", true),
        K::FETCH_KEYWORD_NEXT => ("NEXT ", false),
        K::FETCH_KEYWORD_PRIOR => ("PRIOR ", false),
        K::FETCH_KEYWORD_FIRST => ("FIRST ", false),
        K::FETCH_KEYWORD_LAST => ("LAST ", false),
        K::FETCH_KEYWORD_ABSOLUTE => ("ABSOLUTE ", true),
        K::FETCH_KEYWORD_RELATIVE => ("RELATIVE ", true),
        K::FETCH_KEYWORD_FORWARD => ("FORWARD ", true),
        K::FETCH_KEYWORD_BACKWARD => ("BACKWARD ", true),
        K::FETCH_KEYWORD_ALL => ("ALL ", false),
        K::FETCH_KEYWORD_FORWARD_ALL => ("FORWARD ALL ", false),
        K::FETCH_KEYWORD_BACKWARD_ALL => ("BACKWARD ALL ", false),
        _ => return Err(Error::InvalidField("FetchStmt.direction_keyword")),
    };
    p.text(keyword);
    if counted
        && (s.how_many != 1
            || s.location != Location::UNKNOWN
            || matches!(
                s.direction_keyword,
                K::FETCH_KEYWORD_ABSOLUTE | K::FETCH_KEYWORD_RELATIVE
            ))
    {
        p.text(s.how_many.to_string());
        p.text(" ");
    }
    p.text(named(&s.portalname, "FetchStmt.portalname")?);
    Ok(())
}

fn default_privileges<'a>(s: &'a AlterDefaultPrivilegesStmt, p: &mut Plan<'a>) -> Result<()> {
    p.text("ALTER DEFAULT PRIVILEGES ");
    for n in &s.options {
        let Node::DefElem(d) = n else {
            return Err(Error::InvalidField("AlterDefaultPrivilegesStmt.options"));
        };
        let Some(Node::List(items)) = d.arg.as_deref() else {
            return Err(Error::InvalidField("default privilege names"));
        };
        match d.defname.as_deref() {
            Some("schemas") => {
                p.text("IN SCHEMA ");
                p.text(names(items, ", ")?);
            }
            Some("roles") => {
                p.text("FOR ROLE ");
                super::access::role_list(items, p)?;
            }
            _ => return Err(Error::InvalidField("default privilege option")),
        }
        p.text(" ");
    }
    privileges::grant(required(&s.action, "AlterDefaultPrivilegesStmt.action")?, p)
}

pub(super) fn statement<'a>(n: &'a Node, p: &mut Plan<'a>) -> Result<bool> {
    match n {
        Node::DeclareCursorStmt(s) => declare(s, p)?,
        Node::FetchStmt(s) => fetch(s, p)?,
        Node::AlterDefaultPrivilegesStmt(s) => default_privileges(s, p)?,
        Node::AlterSystemStmt(s) => {
            p.text("ALTER SYSTEM ");
            session::setting(required(&s.setstmt, "AlterSystemStmt.setstmt")?, p)?;
        }
        Node::ConstraintsSetStmt(s) => {
            p.text("SET CONSTRAINTS ");
            if s.constraints.is_empty() {
                p.text("ALL");
            } else {
                relation_list(&s.constraints, p)?;
            }
            p.text(if s.deferred {
                " DEFERRED"
            } else {
                " IMMEDIATE"
            });
        }
        _ => return Ok(false),
    }
    Ok(true)
}
