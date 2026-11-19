//! Raw expression trees produced by PostgreSQL's SQL-syntax function helpers.
use super::{value::Value, Failure};
use pg19_engine::Reduction;
use pg19_nodes::*;

pub(super) enum Arg {
    Node(isize),
    Text(isize),
    Parameter(isize),
    Whitespace(isize),
}
pub(super) enum Args {
    List(isize),
    Items(&'static [Arg]),
}
pub(super) enum Builtin {
    Function {
        name: &'static str,
        system: bool,
        form: CoercionForm,
        args: Args,
        pos: Option<isize>,
        negate: bool,
    },
    SqlValue(SQLValueFunctionOp, Option<isize>),
    Xml {
        op: XmlExprOp,
        name: Option<isize>,
        named: Option<isize>,
        args: Args,
        option: Option<isize>,
    },
    ArgList(Args),
    NullIf,
    XmlRoot(Option<i32>),
    MergeAction,
    SubstringLength,
    Overlaps,
    Treat,
}

fn arguments(args: Args, rhs: &mut Reduction<'_, Value>) -> Result<Vec<Node>, Failure> {
    let items = match args {
        Args::Items(items) => items,
        Args::List(i) => return Ok(rhs.take(i).list()?),
    };
    let mut out = Vec::with_capacity(items.len());
    for arg in items {
        out.push(match *arg {
            Arg::Node(i) => rhs.take(i).node()?,
            Arg::Text(i) => Node::AConst(AConst {
                val: Literal::String(rhs.take(i).text()?),
                location: rhs.location(i),
            }),
            Arg::Parameter(i) => ParamRef {
                number: rhs.take(i).integer()?,
                location: rhs.location(i),
            }
            .into(),
            Arg::Whitespace(i) => Node::AConst(AConst {
                val: Literal::Boolean(rhs.take(i).integer()? != 0),
                location: Location::UNKNOWN,
            }),
        });
    }
    Ok(out)
}

fn function(
    name: &str,
    system: bool,
    args: Vec<Node>,
    form: CoercionForm,
    location: Location,
) -> Node {
    let mut funcname = Vec::new();
    if system {
        funcname.push(Node::String("pg_catalog".into()));
    }
    funcname.push(Node::String(name.into()));
    FuncCall {
        funcname,
        args,
        funcformat: form,
        location,
        ..Default::default()
    }
    .into()
}

#[cfg_attr(not(debug_assertions), inline(always))]
pub(super) fn reduce(op: Builtin, rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let node = match op {
        Builtin::SqlValue(op, typmod) => SQLValueFunction {
            op,
            typmod: typmod
                .map(|i| rhs.take(i).integer())
                .transpose()?
                .unwrap_or(-1),
            location: rhs.location(1),
            ..Default::default()
        }
        .into(),
        Builtin::ArgList(args) => return Ok(Value::List(arguments(args, rhs)?)),
        Builtin::Function {
            name,
            system,
            form,
            args,
            pos,
            negate,
        } => {
            let location = pos.map_or(Location::UNKNOWN, |i| rhs.location(i));
            let call = function(name, system, arguments(args, rhs)?, form, location);
            if negate {
                BoolExpr {
                    boolop: BoolExprType::NOT_EXPR,
                    args: vec![call],
                    location,
                }
                .into()
            } else {
                call
            }
        }
        Builtin::Xml {
            op,
            name,
            named,
            args,
            option,
        } => XmlExpr {
            op,
            name: name.map(|i| rhs.take(i).text()).transpose()?,
            named_args: named
                .map(|i| rhs.take(i).list())
                .transpose()?
                .unwrap_or_default(),
            args: arguments(args, rhs)?,
            xmloption: XmlOptionType(
                option
                    .map(|i| rhs.take(i).integer())
                    .transpose()?
                    .unwrap_or(0),
            ),
            location: rhs.location(1),
            ..Default::default()
        }
        .into(),
        Builtin::XmlRoot(number) => Node::AConst(AConst {
            val: number.map_or(Literal::Null, Literal::Integer),
            location: Location::UNKNOWN,
        }),
        Builtin::MergeAction => MergeSupportFunc {
            msftype: 25,
            location: rhs.location(1),
            ..Default::default()
        }
        .into(),
        Builtin::SubstringLength => {
            let loc = Location::UNKNOWN;
            let target = crate::types::builtin("int4", Vec::new(), loc);
            let length = TypeCast {
                arg: Some(Box::new(rhs.take(3).node()?)),
                type_name: Some(Box::new(target)),
                location: loc,
            };
            return Ok(Value::List(vec![
                rhs.take(1).node()?,
                crate::types::int_constant(1, loc),
                length.into(),
            ]));
        }
        Builtin::NullIf => AExpr {
            kind: AExprKind::AEXPR_NULLIF,
            name: vec![Node::String("=".into())],
            lexpr: Some(Box::new(rhs.take(3).node()?)),
            rexpr: Some(Box::new(rhs.take(5).node()?)),
            location: rhs.location(1),
            ..Default::default()
        }
        .into(),
        Builtin::Overlaps => {
            let mut left = rhs.take(1).list()?;
            let right = rhs.take(3).list()?;
            if left.len() != 2 {
                return Err(Failure::At(
                    "wrong number of parameters on left side of OVERLAPS expression".into(),
                    "42601",
                    rhs.location(1),
                ));
            }
            if right.len() != 2 {
                return Err(Failure::At(
                    "wrong number of parameters on right side of OVERLAPS expression".into(),
                    "42601",
                    rhs.location(3),
                ));
            }
            left.extend(right);
            function(
                "overlaps",
                true,
                left,
                CoercionForm::COERCE_SQL_SYNTAX,
                rhs.location(2),
            )
        }
        Builtin::Treat => {
            let Node::TypeName(typ) = rhs.take(5).node()? else {
                return Err("expected TREAT type".into());
            };
            let Some(Node::String(name)) = typ.names.last() else {
                return Err("expected final type name".into());
            };
            function(
                name,
                true,
                vec![rhs.take(3).node()?],
                CoercionForm::COERCE_EXPLICIT_CALL,
                rhs.location(1),
            )
        }
    };
    Ok(Value::Node(node))
}
