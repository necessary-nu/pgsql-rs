// Raw expression syntax as emitted by libpg_query's postgres_deparse.c.
use super::operator::{binary, builtin, operand, precedence, sublink, Prec};
use super::writer::{names, required, Plan, Role};
use super::{ident, literal, Error, Result};
use pg19_nodes::*;

fn constant(v: &Literal) -> Result<String> {
    match v {
        Literal::Null => Ok("NULL".into()),
        Literal::Integer(n) => Ok(n.to_string()),
        Literal::Boolean(b) => Ok(b.to_string()),
        Literal::Float(n) => super::numeric(n),
        Literal::String(s) => literal(s),
        Literal::BitString(s) => {
            let Some((kind, bits)) = s.split_at_checked(1) else {
                return Err(Error::InvalidField("bit string"));
            };
            if (kind == "b" && bits.bytes().all(|b| matches!(b, b'0' | b'1')))
                || (kind == "x" && bits.bytes().all(|b| b.is_ascii_hexdigit()))
            {
                Ok(format!("{kind}'{bits}'"))
            } else {
                Err(Error::InvalidField("bit string"))
            }
        }
    }
}
pub(super) fn typename<'a>(t: &'a TypeName, p: &mut Plan<'a>) -> Result<()> {
    if t.names.is_empty() {
        return Err(Error::InvalidField("TypeName.names"));
    }
    if t.setof {
        p.text("SETOF ");
    }
    let mut zone = false;
    if let [Node::String(schema), Node::String(name)] = t.names.as_slice() {
        if schema == "pg_catalog" {
            let spelling = match name.as_str() {
                "bpchar" => "char",
                "varchar" => "varchar",
                "numeric" => "numeric",
                "bool" => "boolean",
                "int2" => "smallint",
                "int4" => "int",
                "int8" => "bigint",
                "real" | "float4" => "real",
                "float8" => "double precision",
                "time" => "time",
                "timestamp" => "timestamp",
                "interval" if t.typmods.is_empty() => "interval",
                "interval" => return Err(Error::UnsupportedStatement(Some(NodeTag::TypeName))),
                "timetz" => {
                    zone = true;
                    "time"
                }
                "timestamptz" => {
                    zone = true;
                    "timestamp"
                }
                _ => "",
            };
            if spelling.is_empty() {
                p.text(names(&t.names, ".")?);
            } else {
                p.text(spelling);
            }
        } else {
            p.text(names(&t.names, ".")?);
        }
    } else {
        p.text(names(&t.names, ".")?);
    }
    if !t.typmods.is_empty() {
        p.text(if zone { " (" } else { "(" });
        p.list(&t.typmods, Role::Expr);
        p.text(")");
    }
    if zone {
        p.text(" with time zone");
    }
    for b in &t.array_bounds {
        match b {
            Node::Integer(-1) => p.text("[]"),
            Node::Integer(i) => p.text(format!("[{i}]")),
            _ => return Err(Error::InvalidField("TypeName.arrayBounds")),
        }
    }
    if t.pct_type {
        p.text("%type");
    }
    Ok(())
}
pub(super) fn indirection<'a>(nodes: &'a [Node], p: &mut Plan<'a>) -> Result<()> {
    for n in nodes {
        match n {
            Node::String(s) => {
                p.text(".");
                p.text(ident(s)?);
            }
            Node::AStar(_) => p.text(".*"),
            Node::AIndices(i) => {
                p.text("[");
                if let Some(l) = &i.lidx {
                    p.expr(l);
                }
                if i.is_slice {
                    p.text(":");
                }
                if let Some(u) = &i.uidx {
                    p.expr(u);
                }
                p.text("]");
            }
            _ => return Err(Error::InvalidField("indirection")),
        }
    }
    Ok(())
}
fn boolean<'a>(b: &'a BoolExpr, p: &mut Plan<'a>) -> Result<()> {
    let word = match b.boolop {
        BoolExprType::AND_EXPR if b.args.len() >= 2 => " AND ",
        BoolExprType::OR_EXPR if b.args.len() >= 2 => " OR ",
        BoolExprType::NOT_EXPR if b.args.len() == 1 => {
            p.text("NOT ");
            ""
        }
        _ => return Err(Error::InvalidField("BoolExpr operands")),
    };
    for (i, n) in b.args.iter().enumerate() {
        if i > 0 {
            if b.boolop == BoolExprType::AND_EXPR {
                p.cut();
                p.text("AND ");
            } else {
                p.text(word);
            }
        }
        p.wrapped(n,Role::Expr,matches!(n,Node::BoolExpr(n) if matches!(n.boolop,BoolExprType::AND_EXPR|BoolExprType::OR_EXPR)));
    }
    Ok(())
}
fn cast<'a>(t: &'a TypeCast, p: &mut Plan<'a>) -> Result<()> {
    let arg = required(&t.arg, "TypeCast.arg")?;
    let ty = required(&t.type_name, "TypeCast.typeName")?;
    let negative = matches!(arg,Node::AConst(AConst{val:Literal::Integer(i),..}) if *i<0)
        || matches!(
            arg,
            Node::AConst(AConst {
                val: Literal::Float(_),
                ..
            })
        );
    p.wrapped(arg, Role::Expr, negative || precedence(arg) < Prec::Atom);
    p.text("::");
    p.typename(ty);
    Ok(())
}
fn function<'a>(f: &'a FuncCall, p: &mut Plan<'a>) -> Result<()> {
    if f.funcformat == CoercionForm::COERCE_SQL_SYNTAX {
        return sql_function(f, p);
    }
    if f.funcname.is_empty() {
        return Err(Error::InvalidField("FuncCall.funcname"));
    }
    if f.over.is_some() || f.ignore_nulls != 0 {
        return Err(Error::UnsupportedStatement(Some(NodeTag::FuncCall)));
    }

    p.text(names(&f.funcname, ".")?);
    p.text("(");
    if f.agg_distinct {
        p.text("DISTINCT ");
    }
    if f.agg_star {
        p.text("*");
    }
    for (i, n) in f.args.iter().enumerate() {
        if i != 0 {
            p.text(", ");
        }
        if f.func_variadic && i + 1 == f.args.len() {
            p.text("VARIADIC ");
        }
        p.expr(n);
    }
    if !f.agg_order.is_empty() && !f.agg_within_group {
        p.text(" ORDER BY ");
        p.list(&f.agg_order, Role::Sort);
    }
    p.text(")");
    if f.agg_within_group {
        p.text(" WITHIN GROUP (ORDER BY ");
        p.list(&f.agg_order, Role::Sort);
        p.text(")");
    }
    if let Some(filter) = &f.agg_filter {
        p.text(" FILTER (WHERE ");
        p.expr(filter);
        p.text(")");
    }
    Ok(())
}
fn sql_function<'a>(f: &'a FuncCall, p: &mut Plan<'a>) -> Result<()> {
    match (builtin(f), f.args.as_slice()) {
        (Some("timezone"), [arg]) => {
            operand(p, arg, Prec::At, false);
            p.text(" AT LOCAL");
        }
        (Some("timezone"), [zone, arg]) => {
            operand(p, arg, Prec::At, false);
            p.text(" AT TIME ZONE ");
            operand(p, zone, Prec::At, true);
        }
        (Some("position"), [hay, needle]) => {
            p.text("POSITION(");
            p.wrapped(needle, Role::Expr, precedence(needle) < Prec::Atom);
            p.text(" IN ");
            p.wrapped(hay, Role::Expr, precedence(hay) < Prec::Atom);
            p.text(")");
        }
        (Some("substring"), [arg, from, rest @ ..]) if rest.len() <= 1 => {
            p.text("SUBSTRING(");
            p.expr(arg);
            p.text(" FROM ");
            p.expr(from);
            if let [length] = rest {
                p.text(" FOR ");
                p.expr(length);
            }
            p.text(")");
        }
        (Some("extract"), [field, arg]) => {
            p.text("extract (");
            p.expr(field);
            p.text(" FROM ");
            p.expr(arg);
            p.text(")");
        }
        (Some("system_user"), []) => p.text("SYSTEM_USER"),
        (Some("pg_collation_for"), [arg]) => {
            p.text("collation for (");
            p.expr(arg);
            p.text(")");
        }
        _ => return Err(Error::UnsupportedStatement(Some(NodeTag::FuncCall))),
    }
    Ok(())
}
fn expr_list<'a>(p: &mut Plan<'a>, prefix: &'static str, args: &'a [Node], end: &'static str) {
    p.text(prefix);
    p.list(args, Role::Expr);
    p.text(end);
}
fn value_function(v: &SQLValueFunction, p: &mut Plan<'_>) -> Result<()> {
    let word =
        v.op.pg_name()
            .and_then(|s| s.strip_prefix("SVFOP_"))
            .ok_or(Error::InvalidField("SQLValueFunction.op"))?;
    if let Some(prefix) = word.strip_suffix("_N") {
        p.text(format!("{}({})", prefix.to_ascii_lowercase(), v.typmod));
    } else {
        p.text(word.to_ascii_lowercase());
    }
    Ok(())
}
pub(super) fn expression<'a>(n: &'a Node, p: &mut Plan<'a>) -> Result<()> {
    match n {
        Node::AConst(c) => {
            p.mark(c.location);
            p.text(constant(&c.val)?);
        }
        Node::ColumnRef(c) => {
            p.mark(c.location);
            for (i, f) in c.fields.iter().enumerate() {
                if i > 0 {
                    p.text(".");
                }
                match f {
                    Node::String(s) => p.text(ident(s)?),
                    Node::AStar(_) => p.text("*"),
                    _ => return Err(Error::InvalidField("ColumnRef.fields")),
                }
            }
        }
        Node::ParamRef(v) => {
            p.text(format!("${}", v.number));
        }
        Node::AExpr(a) => return binary(a, p, false),
        Node::BoolExpr(b) => return boolean(b, p),
        Node::TypeCast(t) => return cast(t, p),
        Node::SubLink(s) => return sublink(s, p),
        Node::FuncCall(f) => return function(f, p),
        Node::SQLValueFunction(v) => return value_function(v, p),
        Node::NullTest(t) => {
            operand(p, required(&t.arg, "NullTest.arg")?, Prec::Is, false);
            p.text(match t.nulltesttype {
                NullTestType::IS_NULL => " IS NULL",
                NullTestType::IS_NOT_NULL => " IS NOT NULL",
                _ => return Err(Error::InvalidField("NullTest.nulltesttype")),
            });
        }
        Node::BooleanTest(t) => {
            operand(p, required(&t.arg, "BooleanTest.arg")?, Prec::Is, false);
            let suffix = t
                .booltesttype
                .pg_name()
                .ok_or(Error::InvalidField("BooleanTest.booltesttype"))?;
            p.text(" ");
            p.text(suffix.replace('_', " "));
        }
        Node::AArrayExpr(a) => expr_list(p, "ARRAY[", &a.elements, "]"),
        Node::CoalesceExpr(c) => expr_list(p, "COALESCE(", &c.args, ")"),
        Node::GroupingFunc(g) => expr_list(p, "GROUPING(", &g.args, ")"),
        Node::MinMaxExpr(m) => expr_list(
            p,
            match m.op {
                MinMaxOp::IS_GREATEST => "GREATEST(",
                MinMaxOp::IS_LEAST => "LEAST(",
                _ => return Err(Error::InvalidField("MinMaxExpr.op")),
            },
            &m.args,
            ")",
        ),
        Node::RowExpr(r) => expr_list(
            p,
            match r.row_format {
                CoercionForm::COERCE_EXPLICIT_CALL => "ROW(",
                CoercionForm::COERCE_IMPLICIT_CAST => "(",
                _ => return Err(Error::InvalidField("RowExpr.row_format")),
            },
            &r.args,
            ")",
        ),
        Node::CollateClause(c) => {
            operand(
                p,
                required(&c.arg, "CollateClause.arg")?,
                Prec::Collate,
                false,
            );
            p.text(" COLLATE ");
            p.text(names(&c.collname, ".")?);
        }
        Node::SetToDefault(_) => p.text("DEFAULT"),
        Node::MergeSupportFunc(_) => p.text("merge_action()"),
        Node::CaseExpr(c) => {
            p.text("CASE");
            if let Some(arg) = &c.arg {
                p.text(" ");
                p.expr(arg);
            }
            p.enter();
            for when in &c.args {
                let Node::CaseWhen(w) = when else {
                    return Err(Error::InvalidField("CaseExpr.args"));
                };
                p.clause("WHEN ");
                p.expr(required(&w.expr, "CaseWhen.expr")?);
                p.text(" THEN ");
                p.expr(required(&w.result, "CaseWhen.result")?);
            }
            if let Some(other) = &c.defresult {
                p.clause("ELSE ");
                p.expr(other);
            }
            p.leave();
            p.text(" END");
        }
        Node::NamedArgExpr(a) => {
            p.text(ident(
                a.name
                    .as_deref()
                    .ok_or(Error::InvalidField("NamedArgExpr.name"))?,
            )?);
            p.text(" := ");
            p.expr(required(&a.arg, "NamedArgExpr.arg")?);
        }
        Node::AIndirection(i) => {
            let arg = required(&i.arg, "A_Indirection.arg")?;
            let direct = matches!(arg, Node::ParamRef(_))
                || (matches!(arg, Node::ColumnRef(_))
                    && matches!(i.indirection.first(), Some(Node::AIndices(_))))
                || matches!(arg,Node::SubLink(s) if s.sub_link_type==SubLinkType::EXPR_SUBLINK);
            p.wrapped(arg, Role::Expr, !direct);
            indirection(&i.indirection, p)?;
        }
        _ => return Err(Error::UnsupportedStatement(n.tag())),
    }
    Ok(())
}
