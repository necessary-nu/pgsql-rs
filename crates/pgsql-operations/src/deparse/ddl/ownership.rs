use super::{
    behavior, ident, named, object, relation, relation_name, required, role, Error, Plan, Result,
};
use pgsql_nodes::*;
fn target<'a>(
    kind: ObjectType,
    rel: Option<&RangeVar>,
    obj: Option<&'a Node>,
    p: &mut Plan<'a>,
) -> Result<()> {
    if let Some(r) = rel {
        relation(r, p, false)
    } else {
        object::object(kind, obj.ok_or(Error::InvalidField("object to alter"))?, p)
    }
}
fn header(kind: ObjectType, missing_ok: bool, p: &mut Plan<'_>) -> Result<()> {
    p.text("ALTER ");
    p.text(object::kind_name(kind)?);
    p.text(" ");
    if missing_ok {
        p.text("IF EXISTS ")
    }
    Ok(())
}
fn rename<'a>(s: &'a RenameStmt, p: &mut Plan<'a>) -> Result<()> {
    use ObjectType as O;
    let original = s.rename_type;
    let kind = match original {
        O::OBJECT_ATTRIBUTE => O::OBJECT_TYPE,
        O::OBJECT_COLUMN => s.relation_type,
        O::OBJECT_TABCONSTRAINT => O::OBJECT_TABLE,
        O::OBJECT_DOMCONSTRAINT => O::OBJECT_DOMAIN,
        _ => original,
    };
    header(kind, s.missing_ok, p)?;
    match original {
        O::OBJECT_ATTRIBUTE | O::OBJECT_COLUMN | O::OBJECT_TABCONSTRAINT => {
            let r = required(&s.relation, "RenameStmt.relation")?;
            if original == O::OBJECT_ATTRIBUTE {
                relation_name(r, p)?
            } else {
                relation(r, p, false)?
            }
            p.text(match original {
                O::OBJECT_ATTRIBUTE => " RENAME ATTRIBUTE ",
                O::OBJECT_COLUMN => " RENAME COLUMN ",
                _ => " RENAME CONSTRAINT ",
            });
            p.text(named(&s.subname, "RenameStmt.subname")?);
            p.text(" TO ");
        }
        O::OBJECT_DOMCONSTRAINT => {
            target(kind, None, s.object.as_deref(), p)?;
            p.text(" RENAME CONSTRAINT ");
            p.text(named(&s.subname, "RenameStmt.subname")?);
            p.text(" TO ")
        }
        O::OBJECT_TRIGGER | O::OBJECT_POLICY | O::OBJECT_RULE => {
            p.text(named(&s.subname, "RenameStmt.subname")?);
            p.text(" ON ");
            relation_name(required(&s.relation, "RenameStmt.relation")?, p)?;
            p.text(" RENAME TO ")
        }
        _ => {
            if let Some(name) = &s.subname {
                p.text(ident(name)?)
            } else {
                target(kind, s.relation.as_deref(), s.object.as_deref(), p)?
            }
            p.text(" RENAME TO ");
        }
    }
    p.text(named(&s.newname, "RenameStmt.newname")?);
    behavior(s.behavior, p)
}
fn depends<'a>(s: &'a AlterObjectDependsStmt, p: &mut Plan<'a>) -> Result<()> {
    header(s.object_type, false, p)?;
    if s.object_type == ObjectType::OBJECT_TRIGGER {
        let Some(Node::List(l)) = s.object.as_deref() else {
            return Err(Error::InvalidField("dependent trigger"));
        };
        p.text(super::names(l, ".")?);
        p.text(" ON ");
        relation_name(required(&s.relation, "dependent trigger relation")?, p)?
    } else {
        target(s.object_type, s.relation.as_deref(), s.object.as_deref(), p)?;
    }
    if s.remove {
        p.text(" NO")
    }
    p.text(" DEPENDS ON EXTENSION ");
    p.text(named(&s.extname, "AlterObjectDependsStmt.extname")?);
    Ok(())
}
pub(super) fn statement<'a>(n: &'a Node, p: &mut Plan<'a>) -> Result<bool> {
    match n {
        Node::RenameStmt(s) => rename(s, p)?,
        Node::AlterObjectDependsStmt(s) => depends(s, p)?,
        Node::AlterObjectSchemaStmt(s) => {
            header(s.object_type, s.missing_ok, p)?;
            target(s.object_type, s.relation.as_deref(), s.object.as_deref(), p)?;
            p.text(" SET SCHEMA ");
            p.text(named(&s.newschema, "AlterObjectSchemaStmt.newschema")?);
        }
        Node::AlterOwnerStmt(s) => {
            header(s.object_type, false, p)?;
            target(s.object_type, s.relation.as_deref(), s.object.as_deref(), p)?;
            p.text(" OWNER TO ");
            p.text(role(required(&s.newowner, "AlterOwnerStmt.newowner")?)?);
        }
        _ => return Ok(false),
    }
    Ok(true)
}
