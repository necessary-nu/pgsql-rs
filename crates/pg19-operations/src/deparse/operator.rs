// Operator binding and parenthesization follow src/postgres_deparse.c and PG19 gram.y.
use super::writer::{names, required, Plan, Role};
use super::{Error, Result};
use pg19_nodes::*;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum Prec {
    Or,
    And,
    Not,
    Is,
    Comparison,
    Between,
    Op,
    Add,
    Mul,
    Exponent,
    At,
    Collate,
    Unary,
    Atom,
}
fn single(op: &[Node]) -> Option<&str> {
    match op {
        [Node::String(s)] => Some(s),
        _ => None,
    }
}
fn token_prec(op: &[Node]) -> Prec {
    match single(op) {
        Some("+" | "-") => Prec::Add,
        Some("*" | "/" | "%") => Prec::Mul,
        Some("^") => Prec::Exponent,
        Some("<" | ">" | "=" | "<=" | ">=" | "<>" | "!=") => Prec::Comparison,
        _ => Prec::Op,
    }
}
fn quant_prec(op: &[Node]) -> Prec {
    match single(op) {
        Some("~~" | "!~~" | "~~*" | "!~~*") => Prec::Between,
        _ => token_prec(op),
    }
}
pub(super) fn operator(op: &[Node]) -> Result<String> {
    let Some((Node::String(name), schema)) = op.split_last() else {
        return Err(Error::InvalidField("operator name"));
    };
    if name.is_empty()
        || name.len() > 63
        || name.contains("/*")
        || name.contains("--")
        || !name.bytes().all(|b| b"+-*/<>=~!@#%^&|`?".contains(&b))
    {
        return Err(Error::InvalidField("operator name"));
    }
    if schema.is_empty() {
        Ok(name.clone())
    } else {
        Ok(format!("OPERATOR({}.{})", names(schema, ".")?, name))
    }
}
fn quant_operator(op: &[Node]) -> Result<String> {
    match single(op) {
        Some("~~") => Ok("LIKE".into()),
        Some("!~~") => Ok("NOT LIKE".into()),
        Some("~~*") => Ok("ILIKE".into()),
        Some("!~~*") => Ok("NOT ILIKE".into()),
        _ => operator(op),
    }
}
pub(super) fn builtin(f: &FuncCall) -> Option<&str> {
    match f.funcname.as_slice() {
        [Node::String(schema), Node::String(name)] if schema == "pg_catalog" => Some(name),
        _ => None,
    }
}
fn aexpr_prec(a: &AExpr) -> Prec {
    match a.kind {
        AExprKind::AEXPR_OP if a.lexpr.is_none() => {
            if token_prec(&a.name) == Prec::Add {
                Prec::Unary
            } else {
                Prec::Op
            }
        }
        AExprKind::AEXPR_OP => token_prec(&a.name),
        AExprKind::AEXPR_OP_ANY | AExprKind::AEXPR_OP_ALL => quant_prec(&a.name).min(Prec::Op),
        AExprKind::AEXPR_DISTINCT | AExprKind::AEXPR_NOT_DISTINCT => Prec::Is,
        AExprKind::AEXPR_NULLIF => Prec::Atom,
        _ => Prec::Between,
    }
}
pub(super) fn precedence(n: &Node) -> Prec {
    match n {
        Node::AExpr(a) => aexpr_prec(a),
        Node::BoolExpr(b) => match b.boolop {
            BoolExprType::AND_EXPR => Prec::And,
            BoolExprType::NOT_EXPR => Prec::Not,
            _ => Prec::Or,
        },
        Node::BooleanTest(_) | Node::JsonIsPredicate(_) | Node::NullTest(_) => Prec::Is,
        Node::CollateClause(_) => Prec::Collate,
        Node::SubLink(s)
            if s.sub_link_type == SubLinkType::ANY_SUBLINK && s.oper_name.is_empty() =>
        {
            Prec::Between
        }
        Node::SubLink(s)
            if matches!(
                s.sub_link_type,
                SubLinkType::ANY_SUBLINK | SubLinkType::ALL_SUBLINK
            ) =>
        {
            quant_prec(&s.oper_name).min(Prec::Op)
        }
        Node::FuncCall(f) if f.funcformat == CoercionForm::COERCE_SQL_SYNTAX => match builtin(f) {
            Some("timezone") => Prec::At,
            Some("is_normalized") => Prec::Is,
            _ => Prec::Atom,
        },
        _ => Prec::Atom,
    }
}
fn restricted(n: &Node) -> bool {
    match n {
        Node::AExpr(a) => !matches!(
            a.kind,
            AExprKind::AEXPR_OP
                | AExprKind::AEXPR_DISTINCT
                | AExprKind::AEXPR_NOT_DISTINCT
                | AExprKind::AEXPR_NULLIF
        ),
        Node::FuncCall(f)
            if f.funcformat == CoercionForm::COERCE_SQL_SYNTAX
                && builtin(f) == Some("overlaps") =>
        {
            true
        }
        _ => precedence(n) != Prec::Atom,
    }
}
pub(super) fn operand<'a>(p: &mut Plan<'a>, n: &'a Node, parent: Prec, right: bool) {
    bound(p, n, parent, right, false);
}
fn bound<'a>(p: &mut Plan<'a>, n: &'a Node, parent: Prec, right: bool, b_expr: bool) {
    let child = precedence(n);
    let comparison = matches!(n,Node::AExpr(a) if matches!(a.kind,AExprKind::AEXPR_OP_ANY|AExprKind::AEXPR_OP_ALL|AExprKind::AEXPR_IN))
        || matches!(n,Node::SubLink(s) if matches!(s.sub_link_type,SubLinkType::ANY_SUBLINK|SubLinkType::ALL_SUBLINK));
    let same = match parent {
        Prec::Is | Prec::Comparison | Prec::Between => right || !comparison,
        Prec::Not | Prec::Unary => false,
        _ => right,
    };
    let bracket = child < parent || (child == parent && same) || (b_expr && restricted(n));
    if bracket {
        p.wrapped(n, Role::Expr, true);
    } else if b_expr {
        p.node(n, Role::Restricted);
    } else {
        p.expr(n);
    }
}
pub(super) fn binary<'a>(a: &'a AExpr, p: &mut Plan<'a>, b_expr: bool) -> Result<()> {
    let r = required(&a.rexpr, "A_Expr.rexpr")?;
    let prec = aexpr_prec(a);
    if a.kind == AExprKind::AEXPR_OP {
        if let Some(l) = a.lexpr.as_deref() {
            bound(p, l, prec, false, b_expr);
            p.text(" ");
        }
        p.text(operator(&a.name)?);
        p.text(" ");
        bound(p, r, prec, true, b_expr);
        return Ok(());
    }
    let l = required(&a.lexpr, "A_Expr.lexpr")?;
    if a.kind == AExprKind::AEXPR_NULLIF {
        p.text("NULLIF(");
        p.expr(l);
        p.text(", ");
        p.expr(r);
        p.text(")");
        return Ok(());
    }
    if matches!(a.kind, AExprKind::AEXPR_OP_ANY | AExprKind::AEXPR_OP_ALL) {
        operand(p, l, quant_prec(&a.name), false);
        p.text(" ");
        p.text(quant_operator(&a.name)?);
        p.text(if a.kind == AExprKind::AEXPR_OP_ANY {
            " ANY("
        } else {
            " ALL("
        });
        p.expr(r);
        p.text(")");
        return Ok(());
    }
    bound(p, l, prec, false, b_expr);
    match a.kind {
        AExprKind::AEXPR_DISTINCT => p.text(" IS DISTINCT FROM "),
        AExprKind::AEXPR_NOT_DISTINCT => p.text(" IS NOT DISTINCT FROM "),
        AExprKind::AEXPR_LIKE | AExprKind::AEXPR_ILIKE | AExprKind::AEXPR_SIMILAR => {
            p.text(match single(&a.name) {
                Some("~~") => " LIKE ",
                Some("!~~") => " NOT LIKE ",
                Some("~~*") => " ILIKE ",
                Some("!~~*") => " NOT ILIKE ",
                Some("~") => " SIMILAR TO ",
                Some("!~") => " NOT SIMILAR TO ",
                _ => return Err(Error::InvalidField("pattern operator")),
            });
            if let Node::FuncCall(f) = r {
                if f.funcformat == CoercionForm::COERCE_SQL_SYNTAX
                    && matches!(builtin(f), Some("similar_to_escape" | "like_escape"))
                {
                    let [pattern, end @ ..] = f.args.as_slice() else {
                        return Err(Error::InvalidField("pattern operands"));
                    };
                    operand(p, pattern, prec, true);
                    if let [escape] = end {
                        p.text(" ESCAPE ");
                        operand(p, escape, prec, true);
                    } else if !end.is_empty() {
                        return Err(Error::InvalidField("pattern operands"));
                    }
                    return Ok(());
                }
            }
        }
        AExprKind::AEXPR_IN => {
            p.text(match single(&a.name) {
                Some("=") => " IN (",
                Some("<>") => " NOT IN (",
                _ => return Err(Error::InvalidField("IN operator")),
            });
            let Node::List(items) = r else {
                return Err(Error::InvalidField("IN operands"));
            };
            p.list(items, Role::Expr);
            p.text(")");
            return Ok(());
        }
        AExprKind::AEXPR_BETWEEN
        | AExprKind::AEXPR_NOT_BETWEEN
        | AExprKind::AEXPR_BETWEEN_SYM
        | AExprKind::AEXPR_NOT_BETWEEN_SYM => {
            p.text(match a.kind {
                AExprKind::AEXPR_BETWEEN => " BETWEEN ",
                AExprKind::AEXPR_NOT_BETWEEN => " NOT BETWEEN ",
                AExprKind::AEXPR_BETWEEN_SYM => " BETWEEN SYMMETRIC ",
                _ => " NOT BETWEEN SYMMETRIC ",
            });
            let Node::List(bounds) = r else {
                return Err(Error::InvalidField("BETWEEN operands"));
            };
            let [lower, upper] = bounds.as_slice() else {
                return Err(Error::InvalidField("BETWEEN operands"));
            };
            bound(p, lower, prec, true, true);
            p.text(" AND ");
            operand(p, upper, prec, true);
            return Ok(());
        }
        _ => return Err(Error::InvalidField("A_Expr.kind")),
    }
    bound(p, r, prec, true, b_expr);
    Ok(())
}
pub(super) fn sublink<'a>(s: &'a SubLink, p: &mut Plan<'a>) -> Result<()> {
    match s.sub_link_type {
        SubLinkType::EXISTS_SUBLINK => p.text("EXISTS ("),
        SubLinkType::ARRAY_SUBLINK => p.text("ARRAY("),
        SubLinkType::EXPR_SUBLINK => p.text("("),
        SubLinkType::ANY_SUBLINK if s.oper_name.is_empty() => {
            operand(
                p,
                required(&s.testexpr, "SubLink.testexpr")?,
                Prec::Between,
                false,
            );
            p.text(" IN (");
        }
        SubLinkType::ANY_SUBLINK | SubLinkType::ALL_SUBLINK => {
            operand(
                p,
                required(&s.testexpr, "SubLink.testexpr")?,
                quant_prec(&s.oper_name),
                false,
            );
            p.text(" ");
            p.text(quant_operator(&s.oper_name)?);
            p.text(if s.sub_link_type == SubLinkType::ANY_SUBLINK {
                " ANY ("
            } else {
                " ALL ("
            });
        }
        _ => return Err(Error::InvalidField("SubLink.subLinkType")),
    }
    let Node::SelectStmt(q) = required(&s.subselect, "SubLink.subselect")? else {
        return Err(Error::InvalidField("SubLink.subselect"));
    };
    p.enter();
    p.select(q, false);
    p.leave();
    p.text(")");
    Ok(())
}
