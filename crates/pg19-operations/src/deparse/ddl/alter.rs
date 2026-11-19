use super::{
    behavior, constraints, ident, named, object, relation, required, role, table, Error, Plan,
    Result,
};
use pg19_nodes::*;

fn def(s: &AlterTableCmd) -> Result<&Node> {
    match s.def.as_deref() {
        Some(n) => Ok(n),
        None => Err(Error::InvalidField("AlterTableCmd.def")),
    }
}

fn ident_arg(s: &AlterTableCmd) -> Result<String> {
    match def(s)? {
        Node::String(v) => ident(v),
        _ => Err(Error::InvalidField("AlterTableCmd.def")),
    }
}

fn alter_constraint(c: &ATAlterConstraint, p: &mut Plan<'_>) -> Result<()> {
    p.text("ALTER CONSTRAINT ");
    p.text(named(&c.conname, "ATAlterConstraint.conname")?);
    if c.alter_enforceability {
        p.text(if c.is_enforced {
            " ENFORCED"
        } else {
            " NOT ENFORCED"
        })
    }
    if c.alter_deferrability {
        p.text(if c.initdeferred {
            " INITIALLY DEFERRED"
        } else if c.deferrable {
            " DEFERRABLE"
        } else {
            " NOT DEFERRABLE"
        })
    }
    if c.alter_inheritability {
        p.text(if c.noinherit {
            " NO INHERIT"
        } else {
            " INHERIT"
        })
    }
    Ok(())
}

fn alter_column<'a>(s: &'a AlterTableCmd, attribute: bool, p: &mut Plan<'a>) -> Result<()> {
    use AlterTableType as A;
    p.text(if s.subtype == A::AT_SetIdentity {
        "ALTER "
    } else if attribute {
        "ALTER ATTRIBUTE "
    } else {
        "ALTER COLUMN "
    });
    if s.subtype == A::AT_SetStatistics && s.name.is_none() {
        p.text(s.num.to_string())
    } else {
        p.text(named(&s.name, "AlterTableCmd.name")?)
    }
    match s.subtype {
        A::AT_DropNotNull => p.text(" DROP NOT NULL"),
        A::AT_SetNotNull => p.text(" SET NOT NULL"),
        A::AT_DropExpression => {
            p.text(" DROP EXPRESSION");
            if s.missing_ok {
                p.text(" IF EXISTS")
            }
        }
        A::AT_DropIdentity => {
            p.text(" DROP IDENTITY");
            if s.missing_ok {
                p.text(" IF EXISTS")
            }
        }
        A::AT_ColumnDefault => {
            if let Some(d) = s.def.as_deref() {
                p.text(" SET DEFAULT ");
                p.expr(d)
            } else {
                p.text(" DROP DEFAULT")
            }
        }
        A::AT_SetStatistics => {
            p.text(" SET STATISTICS ");
            if let Some(n) = s.def.as_deref() {
                let Node::Integer(v) = n else {
                    return Err(Error::InvalidField("column statistics"));
                };
                p.text(v.to_string())
            } else {
                p.text("DEFAULT")
            }
        }
        A::AT_SetCompression | A::AT_SetStorage => {
            p.text(if s.subtype == A::AT_SetStorage {
                " SET STORAGE "
            } else {
                " SET COMPRESSION "
            });
            p.text(ident_arg(s)?)
        }
        A::AT_SetExpression => {
            p.text(" SET EXPRESSION AS (");
            p.expr(def(s)?);
            p.text(")")
        }
        A::AT_AlterColumnType => {
            let Node::ColumnDef(d) = def(s)? else {
                return Err(Error::InvalidField("alter column type"));
            };
            p.text(" TYPE ");
            p.typename(required(&d.type_name, "ColumnDef.typeName")?);
            if let Some(c) = &d.coll_clause {
                p.text(" COLLATE ");
                p.text(super::names(&c.collname, ".")?)
            }
            if let Some(expr) = d.raw_default.as_deref() {
                p.text(" USING ");
                p.expr(expr)
            }
        }
        A::AT_SetIdentity => {
            let Node::List(options) = def(s)? else {
                return Err(Error::InvalidField("identity options"));
            };
            p.text(" ");
            super::sequence::options(options, true, p)?;
        }
        A::AT_AddIdentity => {
            let Node::Constraint(c) = def(s)? else {
                return Err(Error::InvalidField("identity constraint"));
            };
            p.text(" ADD ");
            constraints::constraint(c, false, p)?
        }
        A::AT_AlterColumnGenericOptions => {
            let Node::List(opts) = def(s)? else {
                return Err(Error::InvalidField("foreign column options"));
            };
            table::generic_options(opts, p)?
        }
        _ => return Err(Error::InvalidField("alter column operation")),
    }
    Ok(())
}

fn partition<'a>(s: &'a AlterTableCmd, p: &mut Plan<'a>) -> Result<()> {
    let Node::PartitionCmd(cmd) = def(s)? else {
        return Err(Error::InvalidField("alter partition"));
    };
    p.text(if s.subtype == AlterTableType::AT_AttachPartition {
        "ATTACH PARTITION "
    } else {
        "DETACH PARTITION "
    });
    relation(required(&cmd.name, "PartitionCmd.name")?, p, false)?;
    if s.subtype == AlterTableType::AT_AttachPartition {
        if let Some(bound) = cmd.bound.as_deref() {
            p.text(" ");
            table::bound(bound, p)?
        }
    } else if s.subtype == AlterTableType::AT_DetachPartitionFinalize {
        p.text(" FINALIZE")
    } else if cmd.concurrent {
        p.text(" CONCURRENTLY")
    }
    Ok(())
}

fn replica(s: &AlterTableCmd, p: &mut Plan<'_>) -> Result<()> {
    let Node::ReplicaIdentityStmt(r) = def(s)? else {
        return Err(Error::InvalidField("replica identity"));
    };
    p.text("REPLICA IDENTITY ");
    p.text(match r.identity_type {
        b'd' => "DEFAULT",
        b'n' => "NOTHING",
        b'f' => "FULL",
        b'i' => "USING INDEX ",
        _ => return Err(Error::InvalidField("replica identity mode")),
    });
    if r.identity_type == b'i' {
        p.text(named(&r.name, "ReplicaIdentityStmt.name")?)
    }
    Ok(())
}

fn switch(s: &AlterTableCmd, p: &mut Plan<'_>) -> Result<bool> {
    use AlterTableType as A;
    let word = match s.subtype {
        A::AT_EnableTrigAll => "ENABLE TRIGGER ALL",
        A::AT_DisableTrigAll => "DISABLE TRIGGER ALL",
        A::AT_EnableTrigUser => "ENABLE TRIGGER USER",
        A::AT_DisableTrigUser => "DISABLE TRIGGER USER",
        A::AT_SetLogged => "SET LOGGED",
        A::AT_SetUnLogged => "SET UNLOGGED",
        A::AT_DropCluster => "SET WITHOUT CLUSTER",
        A::AT_DropOids => "SET WITHOUT OIDS",
        A::AT_DropOf => "NOT OF",
        A::AT_EnableRowSecurity => "ENABLE ROW LEVEL SECURITY",
        A::AT_DisableRowSecurity => "DISABLE ROW LEVEL SECURITY",
        A::AT_ForceRowSecurity => "FORCE ROW LEVEL SECURITY",
        A::AT_NoForceRowSecurity => "NO FORCE ROW LEVEL SECURITY",
        _ => return Ok(false),
    };
    p.text(word);
    Ok(true)
}

fn with_name(s: &AlterTableCmd, attribute: bool, p: &mut Plan<'_>) -> Result<bool> {
    use AlterTableType as A;
    let word = match s.subtype {
        A::AT_EnableTrig => "ENABLE TRIGGER ",
        A::AT_DisableTrig => "DISABLE TRIGGER ",
        A::AT_EnableAlwaysTrig => "ENABLE ALWAYS TRIGGER ",
        A::AT_EnableReplicaTrig => "ENABLE REPLICA TRIGGER ",
        A::AT_EnableRule => "ENABLE RULE ",
        A::AT_DisableRule => "DISABLE RULE ",
        A::AT_EnableAlwaysRule => "ENABLE ALWAYS RULE ",
        A::AT_EnableReplicaRule => "ENABLE REPLICA RULE ",
        A::AT_ValidateConstraint => "VALIDATE CONSTRAINT ",
        A::AT_DropConstraint => "DROP CONSTRAINT ",
        A::AT_DropColumn if attribute => "DROP ATTRIBUTE ",
        A::AT_DropColumn => "DROP ",
        A::AT_SetTableSpace => "SET TABLESPACE ",
        A::AT_ClusterOn => "CLUSTER ON ",
        _ => return Ok(false),
    };
    p.text(word);
    if s.missing_ok {
        p.text("IF EXISTS ")
    }
    p.text(named(&s.name, "AlterTableCmd.name")?);
    Ok(true)
}

fn command<'a>(s: &'a AlterTableCmd, attribute: bool, p: &mut Plan<'a>) -> Result<()> {
    use AlterTableType as A;
    if !(switch(s, p)? || with_name(s, attribute, p)?) {
        match s.subtype {
            A::AT_AddColumn => {
                p.text(if attribute {
                    "ADD ATTRIBUTE "
                } else {
                    "ADD COLUMN "
                });
                if s.missing_ok {
                    p.text("IF NOT EXISTS ")
                }
                let Node::ColumnDef(c) = def(s)? else {
                    return Err(Error::InvalidField("added column"));
                };
                table::column(c, p)?
            }
            A::AT_AddConstraint => {
                let Node::Constraint(c) = def(s)? else {
                    return Err(Error::InvalidField("added constraint"));
                };
                p.text("ADD ");
                constraints::constraint(c, false, p)?
            }
            A::AT_AlterConstraint => {
                let Node::ATAlterConstraint(c) = def(s)? else {
                    return Err(Error::InvalidField("altered constraint"));
                };
                alter_constraint(c, p)?
            }
            A::AT_ChangeOwner => {
                p.text("OWNER TO ");
                p.text(role(required(&s.newowner, "AlterTableCmd.newowner")?)?)
            }
            A::AT_AttachPartition | A::AT_DetachPartition | A::AT_DetachPartitionFinalize => {
                partition(s, p)?
            }
            A::AT_AddInherit | A::AT_DropInherit => {
                let Node::RangeVar(r) = def(s)? else {
                    return Err(Error::InvalidField("inherit relation"));
                };
                p.text(if s.subtype == A::AT_AddInherit {
                    "INHERIT "
                } else {
                    "NO INHERIT "
                });
                relation(r, p, false)?
            }
            A::AT_AddOf => {
                let Node::TypeName(t) = def(s)? else {
                    return Err(Error::InvalidField("table type"));
                };
                p.text("OF ");
                p.typename(t)
            }
            A::AT_ReplicaIdentity => replica(s, p)?,
            A::AT_SetAccessMethod => {
                p.text("SET ACCESS METHOD ");
                if s.name.is_some() {
                    p.text(named(&s.name, "AlterTableCmd.name")?)
                } else {
                    p.text("DEFAULT")
                }
            }
            A::AT_GenericOptions => {
                let Node::List(opts) = def(s)? else {
                    return Err(Error::InvalidField("foreign options"));
                };
                table::generic_options(opts, p)?;
            }
            _ => alter_column(s, attribute, p)?,
        }
    }
    behavior(s.behavior, p)
}
pub(super) fn alter<'a>(s: &'a AlterTableStmt, p: &mut Plan<'a>) -> Result<()> {
    p.text("ALTER ");
    p.text(object::kind_name(s.objtype)?);
    p.text(" ");
    if s.missing_ok {
        p.text("IF EXISTS ")
    }
    let r = required(&s.relation, "AlterTableStmt.relation")?;
    if s.objtype == ObjectType::OBJECT_TYPE {
        super::relation_name(r, p)?
    } else {
        relation(r, p, false)?
    }
    p.text(" ");
    p.enter();
    for (i, n) in s.cmds.iter().enumerate() {
        if i > 0 {
            p.comma()
        }
        let Node::AlterTableCmd(c) = n else {
            return Err(Error::InvalidField("AlterTableStmt.cmds"));
        };
        command(c, s.objtype == ObjectType::OBJECT_TYPE, p)?;
    }
    p.leave();
    Ok(())
}
