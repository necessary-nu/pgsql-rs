use super::{index::option_value, object};
use crate::deparse::query::named;
use crate::deparse::writer::{names, required, Plan};
use crate::deparse::{ident, literal, Error, Result};
use pg19_nodes::*;
mod define;
mod opclass;

fn qualified(parts: &[Node], field: &'static str) -> Result<String> {
    if parts.is_empty() {
        return Err(Error::InvalidField(field));
    }
    names(parts, ".")
}

fn string_arg(d: &DefElem) -> Result<&str> {
    match d.arg.as_deref() {
        Some(Node::String(s)) => Ok(s),
        _ => Err(Error::InvalidField("DefElem.arg")),
    }
}

fn extension_options(options: &[Node], update: bool, p: &mut Plan<'_>) -> Result<()> {
    for n in options {
        let Node::DefElem(d) = n else {
            return Err(Error::InvalidField("extension options"));
        };
        match (d.defname.as_deref(), update) {
            (Some("schema"), false) => {
                p.text(" SCHEMA ");
                p.text(ident(string_arg(d)?)?);
            }
            (Some("new_version"), _) => {
                p.text(if update { " TO " } else { " VERSION " });
                let v = string_arg(d)?;
                p.text(if v.is_empty() || v.len() > 63 {
                    literal(v)?
                } else {
                    ident(v)?
                });
            }
            (Some("cascade"), false) => p.text(" CASCADE"),
            _ => return Err(Error::InvalidField("extension option")),
        }
    }
    Ok(())
}

fn cast<'a>(s: &'a CreateCastStmt, p: &mut Plan<'a>) -> Result<()> {
    p.text("CREATE CAST (");
    p.typename(required(&s.sourcetype, "CreateCastStmt.sourcetype")?);
    p.text(" AS ");
    p.typename(required(&s.targettype, "CreateCastStmt.targettype")?);
    p.text(") ");
    if let Some(f) = s.func.as_deref() {
        p.text("WITH FUNCTION ");
        object::signature(ObjectType::OBJECT_FUNCTION, f, p)?;
    } else {
        p.text(if s.inout {
            "WITH INOUT"
        } else {
            "WITHOUT FUNCTION"
        });
    }
    p.text(match s.context {
        CoercionContext::COERCION_IMPLICIT => " AS IMPLICIT",
        CoercionContext::COERCION_ASSIGNMENT => " AS ASSIGNMENT",
        CoercionContext::COERCION_EXPLICIT => "",
        _ => return Err(Error::InvalidField("CreateCastStmt.context")),
    });
    Ok(())
}

fn transform<'a>(s: &'a CreateTransformStmt, p: &mut Plan<'a>) -> Result<()> {
    p.text(if s.replace {
        "CREATE OR REPLACE TRANSFORM FOR "
    } else {
        "CREATE TRANSFORM FOR "
    });
    p.typename(required(&s.type_name, "CreateTransformStmt.type_name")?);
    p.text(" LANGUAGE ");
    p.text(named(&s.lang, "CreateTransformStmt.lang")?);
    if s.fromsql.is_none() && s.tosql.is_none() {
        return Err(Error::InvalidField("CreateTransformStmt.fromsql or tosql"));
    }
    p.text(" (");
    if let Some(f) = &s.fromsql {
        p.text("FROM SQL WITH FUNCTION ");
        object::signature(ObjectType::OBJECT_FUNCTION, f, p)?;
    }
    if let Some(f) = &s.tosql {
        if s.fromsql.is_some() {
            p.text(", ");
        }
        p.text("TO SQL WITH FUNCTION ");
        object::signature(ObjectType::OBJECT_FUNCTION, f, p)?;
    }
    p.text(")");
    Ok(())
}

pub(super) fn statement<'a>(n: &'a Node, p: &mut Plan<'a>) -> Result<bool> {
    match n {
        Node::DefineStmt(s) if s.kind != ObjectType::OBJECT_TYPE => define::define(s, p)?,
        Node::AlterTSConfigurationStmt(s) => define::text_config(s, p)?,
        Node::AlterTSDictionaryStmt(s) => {
            p.text("ALTER TEXT SEARCH DICTIONARY ");
            p.text(qualified(&s.dictname, "AlterTSDictionaryStmt.dictname")?);
            p.text(" ");
            parameters(&s.options, p)?;
        }
        Node::CreateExtensionStmt(s) => {
            p.text("CREATE EXTENSION ");
            if s.if_not_exists {
                p.text("IF NOT EXISTS ");
            }
            p.text(named(&s.extname, "CreateExtensionStmt.extname")?);
            extension_options(&s.options, false, p)?;
        }
        Node::AlterExtensionStmt(s) => {
            p.text("ALTER EXTENSION ");
            p.text(named(&s.extname, "AlterExtensionStmt.extname")?);
            p.text(" UPDATE");
            extension_options(&s.options, true, p)?;
        }
        Node::AlterExtensionContentsStmt(s) => {
            p.text("ALTER EXTENSION ");
            p.text(named(&s.extname, "AlterExtensionContentsStmt.extname")?);
            p.text(match s.action {
                1 => " ADD ",
                -1 => " DROP ",
                _ => return Err(Error::InvalidField("AlterExtensionContentsStmt.action")),
            });
            p.text(object::kind_name(s.objtype)?);
            p.text(" ");
            object::object(
                s.objtype,
                required(&s.object, "AlterExtensionContentsStmt.object")?,
                p,
            )?;
        }
        Node::AlterCollationStmt(s) => {
            if s.collname.is_empty() {
                return Err(Error::InvalidField("AlterCollationStmt.collname"));
            }
            p.text("ALTER COLLATION ");
            p.text(names(&s.collname, ".")?);
            p.text(" REFRESH VERSION");
        }
        Node::CreateAmStmt(s) => {
            p.text("CREATE ACCESS METHOD ");
            p.text(named(&s.amname, "CreateAmStmt.amname")?);
            p.text(match s.amtype {
                b'i' => " TYPE INDEX HANDLER ",
                b't' => " TYPE TABLE HANDLER ",
                _ => return Err(Error::InvalidField("CreateAmStmt.amtype")),
            });
            p.text(qualified(&s.handler_name, "CreateAmStmt.handler_name")?);
        }
        Node::CreatePLangStmt(s) => {
            p.text(if s.replace {
                "CREATE OR REPLACE "
            } else {
                "CREATE "
            });
            if s.pltrusted {
                p.text("TRUSTED ");
            }
            p.text("LANGUAGE ");
            p.text(named(&s.plname, "CreatePLangStmt.plname")?);
            if s.plhandler.is_empty() {
                return Err(Error::InvalidField("CreatePLangStmt.plhandler"));
            }
            for (prefix, items) in [
                (" HANDLER ", &s.plhandler),
                (" INLINE ", &s.plinline),
                (" VALIDATOR ", &s.plvalidator),
            ] {
                if !items.is_empty() {
                    p.text(prefix);
                    p.text(names(items, ".")?);
                }
            }
        }
        Node::CreateCastStmt(s) => cast(s, p)?,
        Node::CreateTransformStmt(s) => transform(s, p)?,
        Node::CreateConversionStmt(s) => {
            p.text(if s.def {
                "CREATE DEFAULT CONVERSION "
            } else {
                "CREATE CONVERSION "
            });
            p.text(qualified(
                &s.conversion_name,
                "CreateConversionStmt.conversion_name",
            )?);
            p.text(" FOR ");
            p.text(literal(s.for_encoding_name.as_deref().ok_or(
                Error::InvalidField("CreateConversionStmt.for_encoding_name"),
            )?)?);
            p.text(" TO ");
            p.text(literal(s.to_encoding_name.as_deref().ok_or(
                Error::InvalidField("CreateConversionStmt.to_encoding_name"),
            )?)?);
            p.text("FROM ");
            p.text(qualified(&s.func_name, "CreateConversionStmt.func_name")?);
        }
        Node::CreateOpClassStmt(s) => opclass::class(s, p)?,
        Node::CreateOpFamilyStmt(s) => {
            p.text("CREATE OPERATOR FAMILY ");
            p.text(qualified(&s.opfamilyname, "operator family name")?);
            p.text(" USING ");
            p.text(named(&s.amname, "CreateOpFamilyStmt.amname")?);
        }
        Node::AlterOpFamilyStmt(s) => {
            p.text("ALTER OPERATOR FAMILY ");
            p.text(qualified(&s.opfamilyname, "operator family name")?);
            p.text(" USING ");
            p.text(named(&s.amname, "AlterOpFamilyStmt.amname")?);
            p.text(if s.is_drop { " DROP " } else { " ADD " });
            opclass::items(&s.items, p)?;
        }
        Node::AlterOperatorStmt(s) => {
            p.text("ALTER OPERATOR ");
            object::signature(
                ObjectType::OBJECT_OPERATOR,
                required(&s.opername, "AlterOperatorStmt.opername")?,
                p,
            )?;
            p.text(" SET ");
            operator_options(&s.options, p)?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

pub(super) fn parameters<'a>(items: &'a [Node], p: &mut Plan<'a>) -> Result<()> {
    p.text("(");
    for (i, item) in items.iter().enumerate() {
        let Node::DefElem(option) = item else {
            return Err(Error::InvalidField("definition options"));
        };
        if i > 0 {
            p.text(", ");
        }
        p.mark(option.location);
        p.text(named(&option.defname, "DefElem.defname")?);
        if let Some(value) = option.arg.as_deref() {
            p.text(" = ");
            option_value(value, p)?;
        }
    }
    p.text(")");
    Ok(())
}

pub(super) fn operator_options<'a>(options: &'a [Node], p: &mut Plan<'a>) -> Result<()> {
    p.text("(");
    for (i, n) in options.iter().enumerate() {
        if i > 0 {
            p.text(", ");
        }
        let Node::DefElem(d) = n else {
            return Err(Error::InvalidField("operator options"));
        };
        p.text(named(&d.defname, "DefElem.defname")?);
        p.text(" = ");
        match d.arg.as_deref() {
            None => p.text("NONE"),
            Some(Node::String(s)) if s == "none" => p.text(literal(s)?),
            Some(v) => option_value(v, p)?,
        }
    }
    p.text(")");
    Ok(())
}
