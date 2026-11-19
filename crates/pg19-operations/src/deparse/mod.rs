//! SQL rendering for PostgreSQL 19 raw statement trees.
mod comments;
mod expr;
mod operator;
mod query;
mod writer;
pub use comments::{comments, Comment};

use pg19_codec::protobuf::{statements_from_protobuf, ProtobufError};
use pg19_lexer::{keyword, KeywordKind, Kind, Scanner};
use pg19_nodes::*;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    UnsupportedStatement(Option<NodeTag>),
    InvalidField(&'static str),
    Protobuf(ProtobufError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedStatement(tag) => write!(
                f,
                "unsupported statement {}",
                tag.map_or("NULL", NodeTag::pg_name)
            ),
            Self::InvalidField(field) => write!(f, "invalid raw tree field {field}"),
            Self::Protobuf(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for Error {}

type Result<T> = std::result::Result<T, Error>;

fn plain_name(name: &str) -> bool {
    matches!(name.as_bytes().first(), Some(b'a'..=b'z' | b'_'))
        && name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
}

fn ident(name: &str) -> Result<String> {
    if name.is_empty() || name.len() > 63 || name.contains('\0') {
        return Err(Error::InvalidField("identifier"));
    }
    if plain_name(name) && keyword(name).is_none_or(|(_, _, cat)| cat == KeywordKind::Unreserved) {
        Ok(name.to_owned())
    } else {
        Ok(format!("\"{}\"", name.replace('"', "\"\"")))
    }
}

fn literal(text: &str) -> Result<String> {
    if text.contains('\0') {
        return Err(Error::InvalidField("string literal"));
    }
    let escaped = text.replace('\'', "''");
    if escaped.contains('\\') {
        Ok(format!("E'{}'", escaped.replace('\\', "\\\\")))
    } else {
        Ok(format!("'{escaped}'"))
    }
}

fn numeric(n: &str) -> Result<String> {
    let magnitude = n.strip_prefix('-').unwrap_or(n);
    let token = Scanner::new(magnitude)
        .next()
        .and_then(std::result::Result::ok);
    if token.is_some_and(|t| {
        matches!(t.kind, Kind::FCONST | Kind::ICONST) && t.start == 0 && t.end == magnitude.len()
    }) {
        Ok(n.to_owned())
    } else {
        Err(Error::InvalidField("numeric literal"))
    }
}

fn option_value(arg: &Node) -> Result<String> {
    match arg {
        Node::String(s) => match s.as_str() {
            "true" | "false" | "on" | "off" => Ok(s.to_ascii_uppercase()),
            s if s.is_empty() || s.len() > 63 => literal(s),
            _ => ident(s),
        },
        Node::Integer(n) => Ok(n.to_string()),
        Node::Float(n) => numeric(n),
        _ => Err(Error::InvalidField("DefElem.arg")),
    }
}

fn utility_options(out: &mut String, options: &[Node]) -> Result<()> {
    if options.is_empty() {
        return Ok(());
    }
    out.push_str(" (");
    for (i, option) in options.iter().enumerate() {
        let Node::DefElem(opt) = option else {
            return Err(Error::InvalidField("utility options"));
        };
        if i != 0 {
            out.push_str(", ");
        }
        let name = opt
            .defname
            .as_deref()
            .ok_or(Error::InvalidField("DefElem.defname"))?;
        if plain_name(name)
            && name.len() <= 63
            && (name == "analyze"
                || keyword(name).is_none_or(|(_, _, cat)| cat != KeywordKind::Reserved))
        {
            out.push_str(&name.to_ascii_uppercase());
        } else {
            out.push_str(&ident(name)?);
        }
        if let Some(arg) = &opt.arg {
            out.push(' ');
            out.push_str(&option_value(arg)?);
        }
    }
    out.push(')');
    Ok(())
}

fn repack(out: &mut String, stmt: &RepackStmt) -> Result<()> {
    let cluster = match stmt.command {
        RepackCommand::REPACK_COMMAND_CLUSTER => true,
        RepackCommand::REPACK_COMMAND_REPACK => false,
        _ => return Err(Error::InvalidField("RepackStmt.command")),
    };
    out.push_str(if cluster { "CLUSTER" } else { "REPACK" });
    utility_options(out, &stmt.params)?;
    if let Some(target) = &stmt.relation {
        out.push(' ');
        let relation = target
            .relation
            .as_deref()
            .ok_or(Error::InvalidField("VacuumRelation.relation"))?;
        for part in [&relation.catalogname, &relation.schemaname]
            .into_iter()
            .flatten()
        {
            out.push_str(&ident(part)?);
            out.push('.');
        }
        out.push_str(&ident(
            relation
                .relname
                .as_deref()
                .ok_or(Error::InvalidField("RangeVar.relname"))?,
        )?);
        if !target.va_cols.is_empty() {
            if cluster {
                return Err(Error::InvalidField("VacuumRelation.va_cols"));
            }
            out.push_str(" (");
            for (i, col) in target.va_cols.iter().enumerate() {
                let Node::String(name) = col else {
                    return Err(Error::InvalidField("VacuumRelation.va_cols"));
                };
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&ident(name)?);
            }
            out.push(')');
        }
    }
    if (!stmt.usingindex || stmt.relation.is_none()) && stmt.indexname.is_some()
        || cluster && !stmt.usingindex
    {
        return Err(Error::InvalidField("RepackStmt.usingindex"));
    }
    if !cluster && stmt.usingindex {
        out.push_str(" USING INDEX");
    }
    if let Some(index) = &stmt.indexname {
        out.push_str(if cluster { " USING " } else { " " });
        out.push_str(&ident(index)?);
    }
    Ok(())
}

fn render(out: &mut String, node: &Node) -> Result<()> {
    match node {
        Node::RepackStmt(stmt) => return repack(out, stmt),
        Node::WaitStmt(stmt) => {
            out.push_str("WAIT FOR LSN ");
            out.push_str(&literal(
                stmt.lsn_literal
                    .as_deref()
                    .ok_or(Error::InvalidField("WaitStmt.lsn_literal"))?,
            )?);
            if !stmt.options.is_empty() {
                out.push_str(" WITH");
                utility_options(out, &stmt.options)?;
            }
        }
        Node::CheckPointStmt(stmt) => {
            out.push_str("CHECKPOINT");
            utility_options(out, &stmt.options)?;
        }
        Node::ListenStmt(stmt) => {
            out.push_str("LISTEN ");
            out.push_str(&ident(
                stmt.conditionname
                    .as_deref()
                    .ok_or(Error::InvalidField("ListenStmt.conditionname"))?,
            )?);
        }
        Node::UnlistenStmt(stmt) => {
            out.push_str("UNLISTEN ");
            if let Some(name) = &stmt.conditionname {
                out.push_str(&ident(name)?);
            } else {
                out.push('*');
            }
        }
        Node::NotifyStmt(stmt) => {
            out.push_str("NOTIFY ");
            out.push_str(&ident(
                stmt.conditionname
                    .as_deref()
                    .ok_or(Error::InvalidField("NotifyStmt.conditionname"))?,
            )?);
            if let Some(payload) = &stmt.payload {
                out.push_str(", ");
                out.push_str(&literal(payload)?);
            }
        }
        Node::LoadStmt(stmt) => {
            out.push_str("LOAD ");
            out.push_str(&literal(
                stmt.filename
                    .as_deref()
                    .ok_or(Error::InvalidField("LoadStmt.filename"))?,
            )?);
        }
        Node::DiscardStmt(stmt) => {
            out.push_str(match stmt.target {
                DiscardMode::DISCARD_ALL => "DISCARD ALL",
                DiscardMode::DISCARD_PLANS => "DISCARD PLANS",
                DiscardMode::DISCARD_TEMP => "DISCARD TEMP",
                DiscardMode::DISCARD_SEQUENCES => "DISCARD SEQUENCES",
                _ => return Err(Error::InvalidField("DiscardStmt.target")),
            });
        }
        Node::ClosePortalStmt(stmt) => {
            out.push_str("CLOSE ");
            if let Some(name) = &stmt.portalname {
                out.push_str(&ident(name)?);
            } else {
                out.push_str("ALL");
            }
        }
        Node::DeallocateStmt(stmt) => {
            out.push_str("DEALLOCATE ");
            match (stmt.isall, &stmt.name) {
                (true, None) => out.push_str("ALL"),
                (false, Some(name)) => out.push_str(&ident(name)?),
                _ => return Err(Error::InvalidField("DeallocateStmt.name")),
            }
        }
        _ => return Err(Error::UnsupportedStatement(node.tag())),
    }
    Ok(())
}

/// Render raw statements in compact form. An unsupported node identifies its tag.
// [spec:pgq:req:deparse.stream]
pub fn statements(stmts: &[RawStmt]) -> Result<String> {
    writer::statements(stmts)
}

/// Decode a versioned PG19 ParseResult and render it with the compact writer.
pub fn protobuf(bytes: &[u8]) -> Result<String> {
    statements(&statements_from_protobuf(bytes).map_err(Error::Protobuf)?)
}
