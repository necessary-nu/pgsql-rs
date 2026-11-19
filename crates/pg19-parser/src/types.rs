//! PostgreSQL's raw TypeName constructors and casts from gram.y and makefuncs.c.
use super::{value::Value, Failure};
use pg19_engine::Reduction;
use pg19_nodes::*;

pub(super) enum Modifiers {
    None,
    List(isize),
    Precision(isize),
    IntervalPrecision,
    TimeZoneInterval,
    DefaultOne,
}

pub(super) enum Bounds {
    List(isize),
    Single(Option<isize>),
}

pub(super) enum Types {
    Builtin(&'static str, Modifiers),
    Generic(bool),
    Attribute(bool),
    CharName(Option<isize>),
    Character(Option<isize>),
    Bit(bool),
    Datetime(bool, isize, Modifiers),
    Float(bool),
    Relocate,
    SetModifiers(Modifiers),
    Array {
        set: bool,
        bounds: Bounds,
    },
    Bound(Option<isize>),
    IntervalFields {
        mask: i32,
        seconds: bool,
        precision: Option<isize>,
    },
    Cast {
        arg: isize,
        target: isize,
        pos: isize,
    },
    Constant {
        generic: bool,
        mods: Modifiers,
        value: isize,
        parameter: bool,
    },
}

pub(super) fn named(names: Vec<Node>, typmods: Vec<Node>, location: Location) -> TypeName {
    TypeName {
        names,
        typmods,
        typemod: -1,
        location,
        ..Default::default()
    }
}

pub(super) fn builtin(name: &str, typmods: Vec<Node>, location: Location) -> TypeName {
    named(
        vec![
            Node::String("pg_catalog".into()),
            Node::String(name.to_owned()),
        ],
        typmods,
        location,
    )
}

pub(super) fn int_constant(value: i32, location: Location) -> Node {
    Node::AConst(AConst {
        val: Literal::Integer(value),
        location,
    })
}

fn modifiers(which: Modifiers, rhs: &mut Reduction<'_, Value>) -> Result<Vec<Node>, Failure> {
    Ok(match which {
        Modifiers::None => vec![],
        Modifiers::List(i) => rhs.take(i).list()?,
        Modifiers::TimeZoneInterval => {
            let fields = rhs.take(3).list()?;
            if let Some(first) = fields.first() {
                let Node::AConst(AConst {
                    val: Literal::Integer(mask),
                    ..
                }) = first
                else {
                    return Err("expected interval field mask".into());
                };
                // datetime.h assigns bits 10 and 11 to HOUR and MINUTE.
                if mask & !((1 << 10) | (1 << 11)) != 0 {
                    return Err(Failure::At(
                        "time zone interval must be HOUR or HOUR TO MINUTE".into(),
                        "42601",
                        rhs.location(3),
                    ));
                }
            }
            fields
        }
        Modifiers::Precision(i) => vec![int_constant(rhs.take(i).integer()?, rhs.location(i))],
        Modifiers::IntervalPrecision => {
            vec![
                int_constant(0x7fff, Location::UNKNOWN),
                int_constant(rhs.take(3).integer()?, rhs.location(3)),
            ]
        }
        Modifiers::DefaultOne => vec![int_constant(1, Location::UNKNOWN)],
    })
}

fn typename(value: Value) -> Result<NodeBox<TypeName>, Failure> {
    match value {
        Value::Node(Node::TypeName(n)) => Ok(n),
        _ => Err("expected TypeName".into()),
    }
}

fn bound(rhs: &mut Reduction<'_, Value>, i: Option<isize>) -> Result<Node, Failure> {
    Ok(Node::Integer(
        i.map(|i| rhs.take(i).integer()).transpose()?.unwrap_or(-1),
    ))
}

fn cast(arg: Node, target: Box<TypeName>, location: Location) -> Value {
    Value::Node(
        TypeCast {
            arg: Some(Box::new(arg)),
            type_name: Some(target),
            location,
        }
        .into(),
    )
}

#[cfg_attr(not(debug_assertions), inline(always))]
pub(super) fn reduce(op: Types, rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    // The empty opt_float production has no $1. Every type constructor
    // that records a position starts with its first located token.
    let loc = rhs.lhs_location;
    let result = match op {
        Types::Builtin(name, mods) => builtin(name, modifiers(mods, rhs)?, loc),
        Types::Generic(qualified) => {
            let mut names = vec![Node::String(rhs.take(1).text()?)];
            if qualified {
                names.extend(rhs.take(2).list()?);
            }
            named(names, rhs.take(if qualified { 3 } else { 2 }).list()?, loc)
        }
        Types::Attribute(more) => {
            let mut names = if more {
                rhs.take(1).list()?
            } else {
                Vec::new()
            };
            names.push(Node::String(rhs.take(if more { 3 } else { 2 }).text()?));
            return Ok(Value::List(names));
        }
        Types::CharName(varying) => {
            let varying = varying
                .map(|i| rhs.take(i).integer())
                .transpose()?
                .unwrap_or(1)
                != 0;
            return Ok(Value::Text(
                if varying { "varchar" } else { "bpchar" }.into(),
            ));
        }
        Types::Character(length) => {
            let name = rhs.take(1).text()?;
            let mods = match length {
                Some(i) => Modifiers::Precision(i),
                None if name == "bpchar" => Modifiers::DefaultOne,
                None => Modifiers::None,
            };
            builtin(&name, modifiers(mods, rhs)?, loc)
        }
        Types::Bit(length) => {
            let varying = rhs.take(2).integer()? != 0;
            let mods = match (length, varying) {
                (true, _) => Modifiers::List(4),
                (false, false) => Modifiers::DefaultOne,
                (false, true) => Modifiers::None,
            };
            builtin(
                if varying { "varbit" } else { "bit" },
                modifiers(mods, rhs)?,
                loc,
            )
        }
        Types::Datetime(timestamp, timezone, mods) => {
            let name = match (timestamp, rhs.take(timezone).integer()? != 0) {
                (true, true) => "timestamptz",
                (true, false) => "timestamp",
                (false, true) => "timetz",
                (false, false) => "time",
            };
            builtin(name, modifiers(mods, rhs)?, loc)
        }
        Types::Float(explicit) => {
            let bits = if explicit { rhs.take(2).integer()? } else { 53 };
            let name = match bits {
                1..=24 => "float4",
                25..=53 => "float8",
                _ => {
                    let message = if bits < 1 {
                        "precision for type float must be at least 1 bit"
                    } else {
                        "precision for type float must be less than 54 bits"
                    };
                    return Err(Failure::At(message.into(), "22023", rhs.location(2)));
                }
            };
            builtin(name, Vec::new(), Location::UNKNOWN)
        }
        Types::Relocate => {
            let mut typ = typename(rhs.take(2))?;
            typ.location = loc;
            return Ok(Value::Node(typ.into()));
        }
        Types::SetModifiers(mods) => {
            let mut typ = typename(rhs.take(1))?;
            typ.typmods = modifiers(mods, rhs)?;
            return Ok(Value::Node(typ.into()));
        }
        Types::Array { set, bounds } => {
            let mut typ = typename(rhs.take(if set { 2 } else { 1 }))?;
            typ.array_bounds = match bounds {
                Bounds::List(i) => rhs.take(i).list()?,
                Bounds::Single(i) => vec![bound(rhs, i)?],
            };
            typ.setof = set;
            return Ok(Value::Node(typ.into()));
        }
        Types::Bound(i) => {
            let mut bounds = rhs.take(1).list()?;
            bounds.push(bound(rhs, i)?);
            return Ok(Value::List(bounds));
        }
        Types::IntervalFields {
            mask,
            seconds,
            precision: idx,
        } => {
            let mask = int_constant(mask, loc);
            let mut values = if seconds {
                let mut s = rhs.take(3).list()?;
                s[0] = mask;
                s
            } else {
                vec![mask]
            };
            if let Some(i) = idx {
                values.push(int_constant(rhs.take(i).integer()?, rhs.location(i)));
            }
            return Ok(Value::List(values));
        }
        Types::Cast { arg, target, pos } => {
            return Ok(cast(
                rhs.take(arg).node()?,
                typename(rhs.take(target))?.into_box(),
                rhs.location(pos),
            ))
        }
        Types::Constant {
            generic,
            mods,
            value,
            parameter,
        } => {
            let target = if generic {
                let mods = modifiers(mods, rhs)?;
                if value == 6 {
                    for n in &mods {
                        if let Node::NamedArgExpr(arg) = n {
                            return Err(Failure::At(
                                "type modifier cannot have parameter name".into(),
                                "42601",
                                arg.location,
                            ));
                        }
                    }
                    if !rhs.take(4).list()?.is_empty() {
                        return Err(Failure::At(
                            "type modifier cannot have ORDER BY".into(),
                            "42601",
                            rhs.location(4),
                        ));
                    }
                }
                Box::new(named(rhs.take(1).list()?, mods, loc))
            } else {
                let mut typ = typename(rhs.take(1))?.into_box();
                if !matches!(mods, Modifiers::None) {
                    typ.typmods = modifiers(mods, rhs)?;
                }
                typ
            };
            let pos = rhs.location(value);
            let arg = if parameter {
                ParamRef {
                    number: rhs.take(value).integer()?,
                    location: pos,
                }
                .into()
            } else {
                Node::AConst(AConst {
                    val: Literal::String(rhs.take(value).text()?),
                    location: pos,
                })
            };
            return Ok(cast(arg, target, Location::UNKNOWN));
        }
    };
    Ok(Value::Node(result.into()))
}
