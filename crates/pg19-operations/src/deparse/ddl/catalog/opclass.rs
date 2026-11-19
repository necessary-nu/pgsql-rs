use super::*;
fn types<'a>(values: &'a [Node], p: &mut Plan<'a>) -> Result<()> {
    p.text("(");
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            p.text(", ");
        }
        let Node::TypeName(t) = v else {
            return Err(Error::InvalidField("opclass type list"));
        };
        p.typename(t);
    }
    p.text(")");
    Ok(())
}

pub(super) fn items<'a>(values: &'a [Node], p: &mut Plan<'a>) -> Result<()> {
    if values.is_empty() {
        return Err(Error::InvalidField("opclass items"));
    }
    for (i, n) in values.iter().enumerate() {
        if i > 0 {
            p.text(", ");
        }
        let Node::CreateOpClassItem(s) = n else {
            return Err(Error::InvalidField("opclass items"));
        };
        match s.itemtype {
            1 => {
                p.text(format!("OPERATOR {} ", s.number));
                if let Some(a) = s.name.as_deref() {
                    if a.objargs.is_empty() {
                        let (op, schema) = a
                            .objname
                            .split_last()
                            .ok_or(Error::InvalidField("operator name"))?;
                        if !schema.is_empty() {
                            p.text(names(schema, ".")?);
                            p.text(".");
                        }
                        p.text(crate::deparse::operator::operator(std::slice::from_ref(
                            op,
                        ))?);
                    } else {
                        object::signature(ObjectType::OBJECT_OPERATOR, a, p)?;
                    }
                }
                if !s.order_family.is_empty() {
                    p.text(" FOR ORDER BY ");
                    p.text(names(&s.order_family, ".")?);
                }
                if !s.class_args.is_empty() {
                    types(&s.class_args, p)?;
                }
            }
            2 => {
                p.text(format!("FUNCTION {} ", s.number));
                if !s.class_args.is_empty() {
                    types(&s.class_args, p)?;
                    if s.name.is_some() {
                        p.text(" ");
                    }
                }
                if let Some(a) = s.name.as_deref() {
                    object::signature(ObjectType::OBJECT_FUNCTION, a, p)?;
                }
            }
            3 => {
                p.text("STORAGE ");
                p.typename(required(&s.storedtype, "CreateOpClassItem.storedtype")?);
            }
            _ => return Err(Error::InvalidField("CreateOpClassItem.itemtype")),
        }
    }
    Ok(())
}
pub(super) fn class<'a>(s: &'a CreateOpClassStmt, p: &mut Plan<'a>) -> Result<()> {
    p.text("CREATE OPERATOR CLASS ");
    p.text(qualified(&s.opclassname, "CreateOpClassStmt.opclassname")?);
    if s.is_default {
        p.text(" DEFAULT");
    }
    p.text(" FOR TYPE ");
    p.typename(required(&s.datatype, "CreateOpClassStmt.datatype")?);
    p.text(" USING ");
    p.text(named(&s.amname, "CreateOpClassStmt.amname")?);
    if !s.opfamilyname.is_empty() {
        p.text(" FAMILY ");
        p.text(names(&s.opfamilyname, ".")?);
    }
    p.text(" AS ");
    items(&s.items, p)
}
