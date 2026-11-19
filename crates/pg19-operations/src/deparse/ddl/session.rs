use super::{ident, literal, named, Error, Plan, Result};
use pg19_nodes::*;

fn tx_options<'a>(args: &'a [Node], p: &mut Plan<'a>) -> Result<()> {
    for (i, arg) in args.iter().enumerate() {
        let Node::DefElem(d) = arg else {
            return Err(Error::InvalidField("transaction option"));
        };
        let Some(Node::AConst(v)) = d.arg.as_deref() else {
            return Err(Error::InvalidField("transaction option value"));
        };
        p.text(if i == 0 { " " } else { ", " });
        p.mark(d.location);
        let word = match (d.defname.as_deref(), &v.val) {
            (Some("transaction_isolation"), Literal::String(s)) => match s.as_str() {
                "read committed" => "ISOLATION LEVEL READ COMMITTED",
                "read uncommitted" => "ISOLATION LEVEL READ UNCOMMITTED",
                "repeatable read" => "ISOLATION LEVEL REPEATABLE READ",
                "serializable" => "ISOLATION LEVEL SERIALIZABLE",
                _ => return Err(Error::InvalidField("transaction isolation mode")),
            },
            (Some("transaction_read_only"), Literal::Integer(0)) => "READ WRITE",
            (Some("transaction_read_only"), Literal::Integer(1)) => "READ ONLY",
            (Some("transaction_deferrable"), Literal::Integer(0)) => "NOT DEFERRABLE",
            (Some("transaction_deferrable"), Literal::Integer(1)) => "DEFERRABLE",
            _ => return Err(Error::InvalidField("transaction option value")),
        };
        p.text(word);
    }
    Ok(())
}

pub(super) fn transaction<'a>(s: &'a TransactionStmt, p: &mut Plan<'a>) -> Result<()> {
    use TransactionStmtKind as K;
    p.mark(s.location);
    let prefix = match s.kind {
        K::TRANS_STMT_BEGIN => "BEGIN",
        K::TRANS_STMT_START => "START TRANSACTION",
        K::TRANS_STMT_COMMIT => "COMMIT",
        K::TRANS_STMT_ROLLBACK => "ROLLBACK",
        K::TRANS_STMT_SAVEPOINT => "SAVEPOINT",
        K::TRANS_STMT_RELEASE => "RELEASE",
        K::TRANS_STMT_ROLLBACK_TO => "ROLLBACK TO SAVEPOINT",
        K::TRANS_STMT_PREPARE => "PREPARE TRANSACTION",
        K::TRANS_STMT_COMMIT_PREPARED => "COMMIT PREPARED",
        K::TRANS_STMT_ROLLBACK_PREPARED => "ROLLBACK PREPARED",
        _ => return Err(Error::InvalidField("TransactionStmt.kind")),
    };
    p.text(prefix);
    match s.kind {
        K::TRANS_STMT_BEGIN | K::TRANS_STMT_START => tx_options(&s.options, p)?,
        K::TRANS_STMT_COMMIT | K::TRANS_STMT_ROLLBACK if s.chain => p.text(" AND CHAIN"),
        K::TRANS_STMT_SAVEPOINT | K::TRANS_STMT_RELEASE | K::TRANS_STMT_ROLLBACK_TO => {
            p.text(" ");
            p.text(named(&s.savepoint_name, "TransactionStmt.savepoint_name")?);
        }
        K::TRANS_STMT_PREPARE | K::TRANS_STMT_COMMIT_PREPARED | K::TRANS_STMT_ROLLBACK_PREPARED => {
            p.text(" ");
            p.text(literal(
                s.gid
                    .as_deref()
                    .ok_or(Error::InvalidField("TransactionStmt.gid"))?,
            )?);
        }
        _ => {}
    }
    Ok(())
}

fn var_args<'a>(args: &'a [Node], p: &mut Plan<'a>) -> Result<()> {
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            p.text(", ")
        }
        match arg {
            Node::AConst(AConst {
                val: Literal::String(s),
                location,
            }) => {
                p.mark(*location);
                p.text(match s.as_str() {
                    "true" | "false" | "on" | "off" => s.to_ascii_uppercase(),
                    s if s.is_empty() || s.len() > 63 => literal(s)?,
                    _ => ident(s)?,
                });
            }
            Node::AConst(_) | Node::ParamRef(_) => p.expr(arg),
            Node::TypeCast(_) => return Err(Error::UnsupportedStatement(arg.tag())),
            _ => return Err(Error::InvalidField("VariableSetStmt.args")),
        }
    }
    Ok(())
}

fn time_zone<'a>(args: &'a [Node], p: &mut Plan<'a>) -> Result<()> {
    let [value] = args else {
        return Err(Error::InvalidField("TIME ZONE argument"));
    };
    match value {
        Node::TypeCast(cast) => zone_interval(cast, p),
        Node::AConst(AConst {
            val: Literal::String(s),
            location,
        }) if pg19_lexer::keyword(s).is_some() => {
            p.mark(*location);
            p.text(literal(s)?);
            Ok(())
        }
        _ => var_args(args, p),
    }
}

fn zone_interval<'a>(cast: &'a TypeCast, p: &mut Plan<'a>) -> Result<()> {
    let ty = super::required(&cast.type_name, "time zone interval type")?;
    let value = super::required(&cast.arg, "time zone interval value")?;
    if !matches!(ty.names.as_slice(), [Node::String(schema), Node::String(name)] if schema == "pg_catalog" && name == "interval")
        || !matches!(
            value,
            Node::AConst(AConst {
                val: Literal::String(_),
                ..
            })
        )
    {
        return Err(Error::InvalidField("time zone interval"));
    }
    p.text("interval");
    if let [Node::AConst(AConst {
        val: Literal::Integer(32767),
        ..
    }), Node::AConst(AConst {
        val: Literal::Integer(precision),
        ..
    })] = ty.typmods.as_slice()
    {
        p.text(format!("({precision}) "));
        p.expr(value);
    } else {
        p.text(" ");
        p.expr(value);
        if !ty.typmods.is_empty() {
            crate::deparse::expr::interval_mods(&ty.typmods, p)?;
        }
    }
    Ok(())
}

pub(super) fn setting<'a>(s: &'a VariableSetStmt, p: &mut Plan<'a>) -> Result<()> {
    use VariableSetKind as K;
    if s.kind == K::VAR_RESET_ALL {
        p.text("RESET ALL");
        return Ok(());
    }
    let name = s
        .name
        .as_deref()
        .ok_or(Error::InvalidField("VariableSetStmt.name"))?;
    if s.kind == K::VAR_RESET {
        p.text("RESET ");
        p.text(ident(name)?);
        return Ok(());
    }
    p.text(if s.is_local { "SET LOCAL " } else { "SET " });
    match s.kind {
        K::VAR_SET_VALUE | K::VAR_SET_DEFAULT if name == "timezone" && s.jumble_args => {
            p.text("TIME ZONE ");
            if s.kind == K::VAR_SET_DEFAULT {
                p.text("DEFAULT")
            } else {
                time_zone(&s.args, p)?
            }
        }
        K::VAR_SET_VALUE if name == "xmloption" && s.jumble_args => {
            p.text("XML OPTION ");
            let [Node::AConst(AConst {
                val: Literal::String(word),
                ..
            })] = s.args.as_slice()
            else {
                return Err(Error::InvalidField("XML OPTION"));
            };
            p.text(match word.as_str() {
                "DOCUMENT" => "DOCUMENT",
                "CONTENT" => "CONTENT",
                _ => return Err(Error::InvalidField("XML OPTION")),
            });
        }
        K::VAR_SET_VALUE | K::VAR_SET_DEFAULT => {
            p.text(ident(name)?);
            p.text(" TO ");
            if s.kind == K::VAR_SET_DEFAULT {
                p.text("DEFAULT")
            } else {
                var_args(&s.args, p)?
            }
        }
        K::VAR_SET_CURRENT => {
            p.text(ident(name)?);
            p.text(" FROM CURRENT");
        }
        K::VAR_SET_MULTI => match name {
            "TRANSACTION" | "SESSION CHARACTERISTICS" => {
                p.text(if name == "TRANSACTION" {
                    "TRANSACTION"
                } else {
                    "SESSION CHARACTERISTICS AS TRANSACTION"
                });
                tx_options(&s.args, p)?;
            }
            "TRANSACTION SNAPSHOT" => {
                p.text("TRANSACTION SNAPSHOT ");
                let [Node::AConst(AConst {
                    val: Literal::String(id),
                    ..
                })] = s.args.as_slice()
                else {
                    return Err(Error::InvalidField("transaction snapshot"));
                };
                p.text(literal(id)?);
            }
            _ => return Err(Error::InvalidField("VariableSetStmt.name")),
        },
        _ => return Err(Error::InvalidField("VariableSetStmt.kind")),
    }
    Ok(())
}
