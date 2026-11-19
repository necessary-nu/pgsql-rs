//! PostgreSQL expression operators with qualified names, lists and subqueries.
use super::{value::Value, Failure};
use pg19_engine::Reduction;
use pg19_nodes::*;

pub(super) enum Predicate {
    Qualified(bool),
    Qualify,
    OperatorName(&'static str),
    AnyAll,
    In(bool),
    NotInQuery,
    Between(AExprKind, &'static str, isize),
    SubscriptQuery,
    Array(bool),
    ArrayKeyword,
    Pattern {
        kind: AExprKind,
        op: &'static str,
        helper: &'static str,
        index: isize,
        escape: bool,
    },
    Unique,
    Case,
    When,
}

#[cfg_attr(not(debug_assertions), inline(always))]
pub(super) fn reduce(op: Predicate, rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let node = match op {
        Predicate::Qualified(prefix) => {
            let name = if prefix { 1 } else { 2 };
            AExpr {
                kind: AExprKind::AEXPR_OP,
                name: rhs.take(name).list()?,
                lexpr: if prefix {
                    None
                } else {
                    Some(Box::new(rhs.take(1).node()?))
                },
                rexpr: Some(Box::new(rhs.take(name + 1).node()?)),
                location: rhs.location(name),
                ..Default::default()
            }
            .into()
        }
        Predicate::Qualify => {
            let mut names = vec![Node::String(rhs.take(1).text()?)];
            names.extend(rhs.take(3).list()?);
            return Ok(Value::List(names));
        }
        Predicate::OperatorName(op) => return Ok(Value::List(vec![Node::String(op.into())])),
        Predicate::AnyAll => {
            let which = SubLinkType(rhs.take(3).integer()?);
            AExpr {
                kind: if which == SubLinkType::ANY_SUBLINK {
                    AExprKind::AEXPR_OP_ANY
                } else {
                    AExprKind::AEXPR_OP_ALL
                },
                name: rhs.take(2).list()?,
                lexpr: Some(Box::new(rhs.take(1).node()?)),
                rexpr: Some(Box::new(rhs.take(5).node()?)),
                location: rhs.location(2),
                ..Default::default()
            }
            .into()
        }
        Predicate::In(negated) => {
            let start = if negated { 4 } else { 3 };
            AExpr {
                kind: AExprKind::AEXPR_IN,
                name: vec![Node::String(if negated { "<>" } else { "=" }.into())],
                lexpr: Some(Box::new(rhs.take(1).node()?)),
                rexpr: Some(Box::new(Node::from(rhs.take(start + 1).list()?))),
                rexpr_list_start: rhs.location(start),
                rexpr_list_end: rhs.location(start + 2),
                location: rhs.location(2),
            }
            .into()
        }
        Predicate::NotInQuery => {
            let loc = rhs.location(2);
            let sub = SubLink {
                sub_link_type: SubLinkType::ANY_SUBLINK,
                testexpr: Some(Box::new(rhs.take(1).node()?)),
                subselect: Some(Box::new(rhs.take(4).node()?)),
                location: loc,
                ..Default::default()
            };
            BoolExpr {
                boolop: BoolExprType::NOT_EXPR,
                args: vec![sub.into()],
                location: loc,
            }
            .into()
        }
        Predicate::Between(kind, name, i) => {
            let bounds = vec![rhs.take(i).node()?, rhs.take(i + 2).node()?];
            AExpr {
                kind,
                name: vec![Node::String(name.into())],
                lexpr: Some(Box::new(rhs.take(1).node()?)),
                rexpr: Some(Box::new(Node::from(bounds))),
                location: rhs.location(2),
                ..Default::default()
            }
            .into()
        }
        Predicate::SubscriptQuery => {
            let sub = SubLink {
                sub_link_type: SubLinkType::EXPR_SUBLINK,
                subselect: Some(Box::new(rhs.take(1).node()?)),
                location: rhs.location(1),
                ..Default::default()
            };
            let indirection = rhs.take(2).list()?;
            super::expr::checked_fields(&indirection)?;
            AIndirection {
                arg: Some(Box::new(sub.into())),
                indirection,
            }
            .into()
        }
        Predicate::Array(elements) => {
            let location = rhs.location(1);
            AArrayExpr {
                elements: if elements {
                    rhs.take(2).list()?
                } else {
                    Vec::new()
                },
                list_start: location,
                list_end: rhs.location(if elements { 3 } else { 2 }),
                location,
            }
            .into()
        }
        Predicate::ArrayKeyword => {
            let Node::AArrayExpr(mut array) = rhs.take(2).node()? else {
                return Err("expected array".into());
            };
            array.location = rhs.location(1);
            array.into()
        }
        Predicate::Pattern {
            kind,
            op,
            helper,
            index,
            escape,
        } => {
            let location = rhs.location(2);
            let mut args = vec![rhs.take(index).node()?];
            if escape {
                args.push(rhs.take(index + 2).node()?);
            }
            let call = FuncCall {
                funcname: vec![
                    Node::String("pg_catalog".into()),
                    Node::String(helper.into()),
                ],
                args,
                funcformat: CoercionForm::COERCE_EXPLICIT_CALL,
                location,
                ..Default::default()
            };
            AExpr {
                kind,
                name: vec![Node::String(op.into())],
                lexpr: Some(Box::new(rhs.take(1).node()?)),
                rexpr: Some(Box::new(call.into())),
                location,
                ..Default::default()
            }
            .into()
        }
        Predicate::Unique => {
            return Err(Failure::At(
                "UNIQUE predicate is not yet implemented".into(),
                "0A000",
                rhs.location(1),
            ))
        }
        Predicate::Case => CaseExpr {
            arg: rhs.take(2).optional()?,
            args: rhs.take(3).list()?,
            defresult: rhs.take(4).optional()?,
            location: rhs.location(1),
            ..Default::default()
        }
        .into(),
        Predicate::When => CaseWhen {
            expr: Some(Box::new(rhs.take(2).node()?)),
            result: Some(Box::new(rhs.take(4).node()?)),
            location: rhs.location(1),
        }
        .into(),
    };
    Ok(Value::Node(node))
}
