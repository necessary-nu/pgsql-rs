use super::{behavior, ident, literal, names, required, Error, Plan, Result};
use pg19_nodes::*;

pub(super) fn kind_name(kind: ObjectType) -> Result<&'static str> {
    use ObjectType as O;
    Ok(match kind {
        O::OBJECT_ACCESS_METHOD => "ACCESS METHOD",
        O::OBJECT_AGGREGATE => "AGGREGATE",
        O::OBJECT_ATTRIBUTE => "ATTRIBUTE",
        O::OBJECT_CAST => "CAST",
        O::OBJECT_COLLATION => "COLLATION",
        O::OBJECT_COLUMN => "COLUMN",
        O::OBJECT_CONVERSION => "CONVERSION",
        O::OBJECT_DATABASE => "DATABASE",
        O::OBJECT_DOMAIN => "DOMAIN",
        O::OBJECT_DOMCONSTRAINT => "CONSTRAINT",
        O::OBJECT_EVENT_TRIGGER => "EVENT TRIGGER",
        O::OBJECT_EXTENSION => "EXTENSION",
        O::OBJECT_FDW => "FOREIGN DATA WRAPPER",
        O::OBJECT_FOREIGN_SERVER => "SERVER",
        O::OBJECT_FOREIGN_TABLE => "FOREIGN TABLE",
        O::OBJECT_FUNCTION => "FUNCTION",
        O::OBJECT_INDEX => "INDEX",
        O::OBJECT_LANGUAGE => "LANGUAGE",
        O::OBJECT_LARGEOBJECT => "LARGE OBJECT",
        O::OBJECT_MATVIEW => "MATERIALIZED VIEW",
        O::OBJECT_OPCLASS => "OPERATOR CLASS",
        O::OBJECT_OPERATOR => "OPERATOR",
        O::OBJECT_OPFAMILY => "OPERATOR FAMILY",
        O::OBJECT_PARAMETER_ACL => "PARAMETER",
        O::OBJECT_POLICY => "POLICY",
        O::OBJECT_PROCEDURE => "PROCEDURE",
        O::OBJECT_PUBLICATION => "PUBLICATION",
        O::OBJECT_ROLE => "ROLE",
        O::OBJECT_ROUTINE => "ROUTINE",
        O::OBJECT_RULE => "RULE",
        O::OBJECT_SCHEMA => "SCHEMA",
        O::OBJECT_SEQUENCE => "SEQUENCE",
        O::OBJECT_STATISTIC_EXT => "STATISTICS",
        O::OBJECT_SUBSCRIPTION => "SUBSCRIPTION",
        O::OBJECT_TABCONSTRAINT => "CONSTRAINT",
        O::OBJECT_TABLE => "TABLE",
        O::OBJECT_TABLESPACE => "TABLESPACE",
        O::OBJECT_TRANSFORM => "TRANSFORM",
        O::OBJECT_TRIGGER => "TRIGGER",
        O::OBJECT_TSCONFIGURATION => "TEXT SEARCH CONFIGURATION",
        O::OBJECT_TSDICTIONARY => "TEXT SEARCH DICTIONARY",
        O::OBJECT_TSPARSER => "TEXT SEARCH PARSER",
        O::OBJECT_TSTEMPLATE => "TEXT SEARCH TEMPLATE",
        O::OBJECT_TYPE => "TYPE",
        O::OBJECT_USER_MAPPING => "USER MAPPING",
        O::OBJECT_VIEW => "VIEW",
        _ => return Err(Error::InvalidField("object type")),
    })
}

fn string(n: &Node) -> Result<&str> {
    if let Node::String(s) = n {
        Ok(s)
    } else {
        Err(Error::InvalidField("identifier"))
    }
}

pub(super) fn parameter<'a>(v: &'a FunctionParameter, p: &mut Plan<'a>) -> Result<()> {
    use FunctionParameterMode as M;
    p.mark(v.location);
    let prefix = match v.mode {
        M::FUNC_PARAM_DEFAULT | M::FUNC_PARAM_TABLE => "",
        M::FUNC_PARAM_IN => "IN ",
        M::FUNC_PARAM_OUT => "OUT ",
        M::FUNC_PARAM_INOUT => "INOUT ",
        M::FUNC_PARAM_VARIADIC => "VARIADIC ",
        _ => return Err(Error::InvalidField("FunctionParameter.mode")),
    };
    p.text(prefix);
    if let Some(name) = &v.name {
        p.text(ident(name)?);
        p.text(" ")
    }
    p.typename(required(&v.arg_type, "FunctionParameter.argType")?);
    if let Some(value) = v.defexpr.as_deref() {
        p.text(" = ");
        p.expr(value)
    }
    Ok(())
}

pub(super) fn signature<'a>(
    kind: ObjectType,
    s: &'a ObjectWithArgs,
    p: &mut Plan<'a>,
) -> Result<()> {
    use ObjectType as O;
    if kind == O::OBJECT_OPERATOR {
        let (op, schema) = s
            .objname
            .split_last()
            .ok_or(Error::InvalidField("operator name"))?;
        if !schema.is_empty() {
            p.text(names(schema, ".")?);
            p.text(".")
        }
        p.text(super::super::operator::operator(std::slice::from_ref(op))?);
    } else {
        p.text(names(&s.objname, ".")?)
    }
    if s.args_unspecified {
        return Ok(());
    }
    p.text("(");
    if s.objfuncargs.is_empty() {
        if kind == O::OBJECT_AGGREGATE && s.objargs.is_empty() {
            p.text("*")
        }
        for (i, n) in s.objargs.iter().enumerate() {
            if i > 0 {
                p.text(", ")
            }
            match n {
                Node::Null if kind == O::OBJECT_OPERATOR => p.text("NONE"),
                Node::TypeName(t) => p.typename(t),
                _ => return Err(Error::InvalidField("ObjectWithArgs.objargs")),
            }
        }
    } else {
        for (i, n) in s.objfuncargs.iter().enumerate() {
            if i > 0 {
                p.text(", ")
            }
            let Node::FunctionParameter(v) = n else {
                return Err(Error::InvalidField("ObjectWithArgs.objfuncargs"));
            };
            parameter(v, p)?;
        }
    }
    p.text(")");
    Ok(())
}

pub(super) fn object<'a>(kind: ObjectType, n: &'a Node, p: &mut Plan<'a>) -> Result<()> {
    use ObjectType as O;
    match n {
        Node::String(s) => p.text(ident(s)?),
        Node::Integer(i) if kind == O::OBJECT_LARGEOBJECT => p.text(i.to_string()),
        Node::Float(s) if kind == O::OBJECT_LARGEOBJECT => p.text(super::super::numeric(s)?),
        Node::TypeName(t) => p.typename(t),
        Node::ObjectWithArgs(s) => signature(kind, s, p)?,
        Node::List(l) => match kind {
            O::OBJECT_TABCONSTRAINT | O::OBJECT_TRIGGER | O::OBJECT_RULE | O::OBJECT_POLICY => {
                let (name, target) = l.split_last().ok_or(Error::InvalidField("object name"))?;
                if target.is_empty() {
                    return Err(Error::InvalidField("object target"));
                }
                p.text(ident(string(name)?)?);
                p.text(" ON ");
                p.text(names(target, ".")?);
            }
            O::OBJECT_DOMCONSTRAINT => {
                let [Node::TypeName(t), name] = l.as_slice() else {
                    return Err(Error::InvalidField("domain constraint"));
                };
                p.text(ident(string(name)?)?);
                p.text(" ON DOMAIN ");
                p.typename(t);
            }
            O::OBJECT_TRANSFORM => {
                let [Node::TypeName(t), language] = l.as_slice() else {
                    return Err(Error::InvalidField("transform type and language"));
                };
                p.text("FOR ");
                p.typename(t);
                p.text(" LANGUAGE ");
                p.text(ident(string(language)?)?);
            }
            O::OBJECT_CAST => {
                let [Node::TypeName(a), Node::TypeName(b)] = l.as_slice() else {
                    return Err(Error::InvalidField("cast types"));
                };
                p.text("(");
                p.typename(a);
                p.text(" AS ");
                p.typename(b);
                p.text(")");
            }
            O::OBJECT_OPCLASS | O::OBJECT_OPFAMILY => {
                let (am, name) = l
                    .split_first()
                    .ok_or(Error::InvalidField("access method"))?;
                p.text(names(name, ".")?);
                p.text(" USING ");
                p.text(ident(string(am)?)?);
            }
            _ => p.text(names(l, ".")?),
        },
        _ => return Err(Error::InvalidField("object identity")),
    }
    Ok(())
}

pub(super) fn drop_objects<'a>(s: &'a DropStmt, p: &mut Plan<'a>) -> Result<()> {
    p.text("DROP ");
    p.text(kind_name(s.remove_type)?);
    p.text(" ");
    if s.concurrent {
        p.text("CONCURRENTLY ")
    }
    if s.missing_ok {
        p.text("IF EXISTS ")
    }
    if s.objects.is_empty() {
        return Err(Error::InvalidField("DropStmt.objects"));
    }
    for (i, obj) in s.objects.iter().enumerate() {
        if i > 0 {
            p.text(", ")
        }
        object(s.remove_type, obj, p)?;
    }
    behavior(s.behavior, p)
}

pub(super) fn label<'a>(
    kind: ObjectType,
    target: &'a Option<Box<Node>>,
    label: &Option<String>,
    p: &mut Plan<'a>,
) -> Result<()> {
    p.text(kind_name(kind)?);
    p.text(" ");
    object(kind, required(target, "object to label")?, p)?;
    p.text(" IS ");
    p.text(if let Some(label) = label {
        literal(label)?
    } else {
        "NULL".to_owned()
    });
    Ok(())
}
