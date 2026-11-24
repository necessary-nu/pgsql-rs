//! Expression constructors from PostgreSQL gram.y and nodes/makefuncs.c.
use super::{value::Value, Failure};
use crate::raw::*;
use pgsql_engine::Reduction;

pub(super) enum Expr {
    Integer,
    Float,
    String,
    Bits,
    Boolean(bool),
    Null,
    Operator(AExprKind, &'static str, isize),
    Positive,
    Negative,
    Logic(BoolExprType),
    Document(bool),
    NullTest(NullTestType),
    BooleanTest(BoolTestType),
    Column(bool),
    Parameter,
    Parentheses,
    Field,
    AppendField,
    Star,
    Index,
    Slice,
}

// The LALR reductions already encode PostgreSQL precedence. Their left and
// right values become children without reassociating the tree.
// [spec:pgq:req:grammar.expression-binding]
fn operator(
    kind: AExprKind,
    name: &str,
    left: Option<Node>,
    right: Node,
    location: Location,
) -> Node {
    AExpr {
        kind,
        name: vec![Node::String(name.into())],
        lexpr: left.map(Box::new),
        rexpr: Some(Box::new(right)),
        location,
        ..Default::default()
    }
    .into()
}

fn negate(mut value: Node, location: Location) -> Node {
    if let Node::AConst(c) = &mut value {
        c.location = location;
        match &mut c.val {
            Literal::Integer(v) => {
                *v = v.wrapping_neg();
                return value;
            }
            Literal::Float(v) => {
                let digits = v.strip_prefix('+').unwrap_or(v);
                *v = match digits.strip_prefix('-') {
                    Some(unsigned) => unsigned.into(),
                    None => format!("-{digits}").into(),
                };
                return value;
            }
            _ => {}
        }
    }
    operator(AExprKind::AEXPR_OP, "-", None, value, location)
}

fn logic(which: BoolExprType, rhs: &mut Reduction<'_, Value>) -> Result<Node, Failure> {
    if which == BoolExprType::NOT_EXPR {
        return Ok(BoolExpr {
            boolop: which,
            args: vec![rhs.take(2).node()?],
            location: rhs.location(1),
        }
        .into());
    }
    let mut left = rhs.take(1).node()?;
    let right = rhs.take(3).node()?;
    if let Node::BoolExpr(existing) = &mut left {
        if existing.boolop == which {
            existing.args.push(right);
            return Ok(left);
        }
    }
    Ok(BoolExpr {
        boolop: which,
        args: vec![left, right],
        location: rhs.location(2),
    }
    .into())
}

pub(super) fn checked_fields(fields: &[Node]) -> Result<(), Failure> {
    if fields
        .iter()
        .rev()
        .skip(1)
        .any(|n| matches!(n, Node::AStar(_)))
    {
        Err(Failure::Syntax("improper use of \"*\""))
    } else {
        Ok(())
    }
}

fn indirect(base: Node, fields: Vec<Node>) -> Node {
    if fields.is_empty() {
        base
    } else {
        AIndirection {
            arg: Some(Box::new(base)),
            indirection: fields,
        }
        .into()
    }
}

fn column(name: String, mut fields: Vec<Node>, location: Location) -> Result<Node, Failure> {
    checked_fields(&fields)?;
    let tail = fields
        .iter()
        .position(|n| matches!(n, Node::AIndices(_)))
        .map_or_else(Vec::new, |i| fields.split_off(i));
    if fields.is_empty() {
        fields = vec![Node::String(name)];
    } else {
        fields.insert(0, Node::String(name));
    }
    Ok(indirect(ColumnRef { fields, location }.into(), tail))
}

#[cfg_attr(not(debug_assertions), inline(always))]
pub(super) fn reduce(action: Expr, rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let location = rhs.location(1);
    let constant = |val| Node::AConst(AConst { val, location });
    let result = match action {
        Expr::Integer => constant(Literal::Integer(rhs.take(1).integer()?)),
        Expr::Float => constant(Literal::Float(rhs.take(1).text()?)),
        Expr::String => constant(Literal::String(rhs.take(1).text()?)),
        Expr::Bits => constant(Literal::BitString(rhs.take(1).text()?)),
        Expr::Boolean(v) => constant(Literal::Boolean(v)),
        Expr::Null => constant(Literal::Null),
        Expr::Operator(kind, name, index) => operator(
            kind,
            name,
            Some(rhs.take(1).node()?),
            rhs.take(index).node()?,
            rhs.location(2),
        ),
        Expr::Positive => operator(
            AExprKind::AEXPR_OP,
            "+",
            None,
            rhs.take(2).node()?,
            location,
        ),
        Expr::Negative => negate(rhs.take(2).node()?, location),
        Expr::Logic(which) => logic(which, rhs)?,
        Expr::Document(negate) => {
            let location = rhs.location(2);
            let document = XmlExpr {
                op: XmlExprOp::IS_DOCUMENT,
                args: vec![rhs.take(1).node()?],
                location,
                ..Default::default()
            }
            .into();
            if negate {
                BoolExpr {
                    boolop: BoolExprType::NOT_EXPR,
                    args: vec![document],
                    location,
                }
                .into()
            } else {
                document
            }
        }
        Expr::NullTest(which) => NullTest {
            arg: Some(Box::new(rhs.take(1).node()?)),
            nulltesttype: which,
            argisrow: false,
            location: rhs.location(2),
        }
        .into(),
        Expr::BooleanTest(which) => BooleanTest {
            arg: Some(Box::new(rhs.take(1).node()?)),
            booltesttype: which,
            location: rhs.location(2),
        }
        .into(),
        Expr::Column(has_fields) => {
            let name = rhs.take(1).text()?;
            match has_fields.then(|| rhs.take(2)) {
                None => ColumnRef {
                    fields: vec![Node::String(name)],
                    location,
                }
                .into(),
                Some(Value::Node(Node::String(field))) => ColumnRef {
                    fields: vec![Node::String(name), Node::String(field)],
                    location,
                }
                .into(),
                Some(fields) => column(name, fields.indirection()?, location)?,
            }
        }
        Expr::Parameter => {
            let fields = rhs.take(2).list()?;
            checked_fields(&fields)?;
            indirect(
                ParamRef {
                    number: rhs.take(1).integer()?,
                    location,
                }
                .into(),
                fields,
            )
        }
        Expr::Parentheses => {
            let fields = rhs.take(4).list()?;
            checked_fields(&fields)?;
            indirect(rhs.take(2).node()?, fields)
        }
        Expr::Field => Node::String(rhs.take(2).text()?),
        Expr::AppendField => {
            let next = rhs.take(2).node()?;
            let fields = match rhs.take(1) {
                Value::Node(first) => vec![first, next],
                Value::List(mut fields) => {
                    fields.push(next);
                    fields
                }
                _ => return Err("expected indirection".into()),
            };
            return Ok(Value::from_list(fields));
        }
        Expr::Star => AStar {}.into(),
        Expr::Index => AIndices {
            is_slice: false,
            lidx: None,
            uidx: Some(Box::new(rhs.take(2).node()?)),
        }
        .into(),
        Expr::Slice => AIndices {
            is_slice: true,
            lidx: rhs.take(2).optional()?,
            uidx: rhs.take(4).optional()?,
        }
        .into(),
    };
    Ok(Value::from_node(result))
}
