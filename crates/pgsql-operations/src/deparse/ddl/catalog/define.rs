use super::*;

fn arguments<'a>(args: &'a [Node], p: &mut Plan<'a>) -> Result<()> {
    let [list, Node::Integer(order)] = args else {
        return Err(Error::InvalidField("DefineStmt.args"));
    };
    p.text("(");
    match list {
        Node::Null => p.text("*"),
        Node::List(list) => {
            for (i, n) in list.iter().enumerate() {
                if i as i32 == *order {
                    if i > 0 {
                        p.text(" ");
                    }
                    p.text("ORDER BY ");
                } else if i > 0 {
                    p.text(", ");
                }
                let Node::FunctionParameter(a) = n else {
                    return Err(Error::InvalidField("aggregate parameter"));
                };
                object::parameter(a, p)?;
            }
            if *order == list.len() as i32 {
                let Some(Node::FunctionParameter(a)) = list.last() else {
                    return Err(Error::InvalidField("aggregate ordered parameter"));
                };
                p.text(" ORDER BY ");
                object::parameter(a, p)?;
            }
        }
        _ => return Err(Error::InvalidField("aggregate argument list")),
    }
    p.text(")");
    Ok(())
}

pub(super) fn define<'a>(s: &'a DefineStmt, p: &mut Plan<'a>) -> Result<()> {
    use ObjectType as O;
    p.text(if s.replace {
        "CREATE OR REPLACE "
    } else {
        "CREATE "
    });
    p.text(object::kind_name(s.kind)?);
    p.text(if s.if_not_exists {
        " IF NOT EXISTS "
    } else {
        " "
    });
    if s.kind == O::OBJECT_OPERATOR {
        let (op, ns) = s
            .defnames
            .split_last()
            .ok_or(Error::InvalidField("DefineStmt.defnames"))?;
        if !ns.is_empty() {
            p.text(names(ns, ".")?);
            p.text(".");
        }
        p.text(crate::deparse::operator::operator(std::slice::from_ref(
            op,
        ))?);
    } else {
        p.text(qualified(&s.defnames, "DefineStmt.defnames")?);
    }
    if !s.oldstyle && s.kind == O::OBJECT_AGGREGATE {
        p.text(" ");
        arguments(&s.args, p)?;
    }
    if s.kind == O::OBJECT_COLLATION {
        if let [Node::DefElem(d)] = s.definition.as_slice() {
            if d.defname.as_deref() == Some("from") {
                let Some(Node::List(name)) = d.arg.as_deref() else {
                    return Err(Error::InvalidField("collation source"));
                };
                p.text(" FROM ");
                p.text(qualified(name, "collation source")?);
                return Ok(());
            }
        }
    }
    if !s.definition.is_empty() {
        p.text(" ");
        parameters(&s.definition, p)?;
    }
    Ok(())
}

pub(super) fn text_config(s: &AlterTSConfigurationStmt, p: &mut Plan<'_>) -> Result<()> {
    use AlterTSConfigType as T;
    p.text("ALTER TEXT SEARCH CONFIGURATION ");
    p.text(qualified(&s.cfgname, "AlterTSConfigurationStmt.cfgname")?);
    p.text(match s.kind {
        T::ALTER_TSCONFIG_ADD_MAPPING => " ADD MAPPING FOR ",
        T::ALTER_TSCONFIG_ALTER_MAPPING_FOR_TOKEN | T::ALTER_TSCONFIG_REPLACE_DICT_FOR_TOKEN => {
            " ALTER MAPPING FOR "
        }
        T::ALTER_TSCONFIG_REPLACE_DICT => " ALTER MAPPING",
        T::ALTER_TSCONFIG_DROP_MAPPING if s.missing_ok => " DROP MAPPING IF EXISTS FOR ",
        T::ALTER_TSCONFIG_DROP_MAPPING => " DROP MAPPING FOR ",
        _ => return Err(Error::InvalidField("AlterTSConfigurationStmt.kind")),
    });
    p.text(names(&s.tokentype, ", ")?);
    if s.kind == T::ALTER_TSCONFIG_DROP_MAPPING {
        return Ok(());
    }
    let replace = matches!(
        s.kind,
        T::ALTER_TSCONFIG_REPLACE_DICT | T::ALTER_TSCONFIG_REPLACE_DICT_FOR_TOKEN
    );
    if replace && s.dicts.len() != 2 {
        return Err(Error::InvalidField("replacement dictionaries"));
    }
    for (i, n) in s.dicts.iter().enumerate() {
        let Node::List(n) = n else {
            return Err(Error::InvalidField("dictionary name"));
        };
        p.text(match (replace, i) {
            (true, 0) => " REPLACE ",
            (true, 1) | (false, 0) => " WITH ",
            _ => ", ",
        });
        p.text(qualified(n, "dictionary name")?);
    }
    Ok(())
}
