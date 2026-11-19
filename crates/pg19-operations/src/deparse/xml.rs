// SQL/XML syntax has both expression and table-reference productions in gram.y.
use super::operator::{operand, precedence, Prec};
use super::query::{alias, named};
use super::writer::{required, Plan, Role};
use super::{ident, Error, Result};
use pg19_nodes::*;

fn kind(option: XmlOptionType) -> Result<&'static str> {
    match option {
        XmlOptionType::XMLOPTION_DOCUMENT => Ok("document "),
        XmlOptionType::XMLOPTION_CONTENT => Ok("content "),
        _ => Err(Error::InvalidField("XML option")),
    }
}
fn attributes<'a>(args: &'a [Node], namespace: bool, p: &mut Plan<'a>) -> Result<()> {
    for (i, n) in args.iter().enumerate() {
        let Node::ResTarget(t) = n else {
            return Err(Error::InvalidField("XML target"));
        };
        if i != 0 {
            p.text(", ");
        }
        if namespace && t.name.is_none() {
            p.text("DEFAULT ");
        }
        let val = required(&t.val, "ResTarget.val")?;
        if namespace {
            super::operator::b_expr(val, p);
        } else {
            p.expr(val);
        }
        if let Some(name) = &t.name {
            p.text(" AS ");
            p.text(ident(name)?);
        }
    }
    Ok(())
}
pub(super) fn expression<'a>(x: &'a XmlExpr, p: &mut Plan<'a>) -> Result<()> {
    match (x.op, x.args.as_slice()) {
        (XmlExprOp::IS_XMLCONCAT, args) => {
            p.text("xmlconcat(");
            p.list(args, Role::Expr);
        }
        (XmlExprOp::IS_XMLELEMENT, args) => {
            p.text("xmlelement(name ");
            p.text(named(&x.name, "XmlExpr.name")?);
            if !x.named_args.is_empty() {
                p.text(", xmlattributes(");
                attributes(&x.named_args, false, p)?;
                p.text(")");
            }
            if !args.is_empty() {
                p.text(", ");
                p.list(args, Role::Expr);
            }
        }
        (XmlExprOp::IS_XMLFOREST, []) => {
            p.text("xmlforest(");
            attributes(&x.named_args, false, p)?;
        }
        (XmlExprOp::IS_XMLPI, args) if args.len() <= 1 => {
            p.text("xmlpi(name ");
            p.text(named(&x.name, "XmlExpr.name")?);
            if let [arg] = args {
                p.text(", ");
                p.expr(arg);
            }
        }
        (
            XmlExprOp::IS_XMLPARSE,
            [arg, Node::AConst(AConst {
                val: Literal::Boolean(preserve),
                ..
            })],
        ) => {
            p.text("xmlparse(");
            p.text(kind(x.xmloption)?);
            p.expr(arg);
            if *preserve {
                p.text(" PRESERVE WHITESPACE");
            }
        }
        (
            XmlExprOp::IS_XMLROOT,
            [doc, version, Node::AConst(AConst {
                val: Literal::Integer(standalone),
                ..
            })],
        ) => {
            p.text("xmlroot(");
            p.expr(doc);
            p.text(", version ");
            if matches!(
                version,
                Node::AConst(AConst {
                    val: Literal::Null,
                    ..
                })
            ) {
                p.text("no value");
            } else {
                p.expr(version);
            }
            p.text(match standalone {
                0 => ", standalone yes",
                1 => ", standalone no",
                2 => ", standalone no value",
                3 => "",
                _ => return Err(Error::InvalidField("XMLROOT standalone")),
            });
        }
        (XmlExprOp::IS_DOCUMENT, [doc]) => {
            operand(p, doc, Prec::Is, false);
            p.text(" IS DOCUMENT");
            return Ok(());
        }
        _ => return Err(Error::InvalidField("XmlExpr operands")),
    }
    p.text(")");
    Ok(())
}
pub(super) fn serialize<'a>(x: &'a XmlSerialize, p: &mut Plan<'a>) -> Result<()> {
    p.text("xmlserialize(");
    p.text(kind(x.xmloption)?);
    p.expr(required(&x.expr, "XmlSerialize.expr")?);
    p.text(" AS ");
    p.typename(required(&x.type_name, "XmlSerialize.typeName")?);
    if x.indent {
        p.text(" INDENT");
    }
    p.text(")");
    Ok(())
}
pub(super) fn table<'a>(t: &'a RangeTableFunc, p: &mut Plan<'a>) -> Result<()> {
    if t.lateral {
        p.text("LATERAL ");
    }
    p.text("xmltable(");
    if !t.namespaces.is_empty() {
        p.text("xmlnamespaces(");
        attributes(&t.namespaces, true, p)?;
        p.text("), ");
    }
    p.text("(");
    p.expr(required(&t.rowexpr, "RangeTableFunc.rowexpr")?);
    p.text(") PASSING ");
    let doc = required(&t.docexpr, "RangeTableFunc.docexpr")?;
    p.wrapped(doc, Role::Expr, precedence(doc) < Prec::Atom);
    p.text(" COLUMNS ");
    for (i, n) in t.columns.iter().enumerate() {
        let Node::RangeTableFuncCol(c) = n else {
            return Err(Error::InvalidField("RangeTableFunc.columns"));
        };
        if i > 0 {
            p.text(", ");
        }
        p.text(named(&c.colname, "RangeTableFuncCol.colname")?);
        p.text(" ");
        if c.for_ordinality {
            p.text("FOR ORDINALITY");
        } else {
            p.typename(required(&c.type_name, "RangeTableFuncCol.typeName")?);
            if let Some(path) = &c.colexpr {
                p.text(" PATH ");
                super::operator::b_expr(path, p);
            }
            if let Some(default) = &c.coldefexpr {
                p.text(" DEFAULT ");
                super::operator::b_expr(default, p);
            }
            if c.is_not_null {
                p.text(" NOT NULL");
            }
        }
    }
    p.text(")");
    if let Some(a) = &t.alias {
        p.text(" AS ");
        p.text(alias(a)?);
    }
    Ok(())
}
