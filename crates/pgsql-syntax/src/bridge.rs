//! Convert between the owned native raw records and the public prost messages.
//! Both representations are typed. SQL and protobuf bytes are unnecessary here.
use crate::{protobuf as pb, Error, NodeEnum, NodeRef, Result, PG_VERSION_NUM};
use pgsql_parser::nodes as n;

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

#[derive(Clone, Copy)]
struct Depth(u8);

impl Depth {
    const ROOT: Self = Self(100);

    // Count embedded messages, including the Node oneof, as prost does.
    fn child(self) -> Result<Self> {
        self.0
            .checked_sub(1)
            .map(Self)
            .ok_or_else(|| Error::Decode(prost::DecodeError::new("recursion limit reached")))
    }
}

trait Record: Sized {
    type Proto;
    fn to_proto(&self, depth: Depth) -> Result<Self::Proto>;
    fn from_proto(value: &Self::Proto, depth: Depth) -> Result<Self>;
}

pub(crate) fn parse_tree(stmts: &[n::RawStmt]) -> Result<pb::ParseResult> {
    Ok(pb::ParseResult {
        version: PG_VERSION_NUM,
        stmts: stmts
            .iter()
            .map(|s| s.to_proto(Depth::ROOT))
            .collect::<Result<_>>()?,
    })
}

pub(crate) fn statements(tree: &pb::ParseResult) -> Result<Vec<n::RawStmt>> {
    if tree.version != 0 {
        super::check_version(tree.version)?;
    }
    tree.stmts
        .iter()
        .map(|s| n::RawStmt::from_proto(s, Depth::ROOT))
        .collect()
}

pub(crate) fn statement(node: NodeRef<'_>) -> Result<n::RawStmt> {
    Ok(n::RawStmt::new(
        native_node(node, Depth::ROOT.child()?.child()?)?,
        Default::default(),
    ))
}

trait Scalar<P>: Sized {
    fn export(&self) -> Result<P>;
    fn import(p: &P) -> Result<Self>;
}

macro_rules! simple {
    ($($ty:ty),*) => { $(impl Scalar<$ty> for $ty {
        fn export(&self) -> Result<Self> { Ok(*self) }
        fn import(p: &Self) -> Result<Self> { Ok(*p) }
    })* };
}
simple!(u32, u64, i32, i64, bool, f64);

impl Scalar<i32> for i16 {
    fn export(&self) -> Result<i32> {
        Ok(i32::from(*self))
    }
    fn import(p: &i32) -> Result<Self> {
        Self::try_from(*p).map_err(|_| Error::Parse("int16 field out of range".into()))
    }
}
impl Scalar<i32> for n::Location {
    fn export(&self) -> Result<i32> {
        Ok(self.as_pg())
    }
    fn import(p: &i32) -> Result<Self> {
        Self::from_pg(*p).ok_or_else(|| Error::Parse("invalid byte location".into()))
    }
}
impl Scalar<String> for u8 {
    fn export(&self) -> Result<String> {
        Ok(if *self == 0 {
            String::new()
        } else {
            char::from(*self).to_string()
        })
    }
    fn import(p: &String) -> Result<Self> {
        let mut chars = p.chars();
        let value = chars.next().unwrap_or('\0');
        u8::try_from(value)
            .ok()
            .filter(|_| chars.next().is_none())
            .ok_or_else(|| Error::Parse("expected one byte character".into()))
    }
}
impl Scalar<String> for Option<String> {
    fn export(&self) -> Result<String> {
        Ok(self.clone().unwrap_or_default())
    }
    fn import(p: &String) -> Result<Self> {
        Ok((!p.is_empty()).then(|| p.clone()))
    }
}
impl Scalar<Vec<u64>> for n::Bitmapset {
    fn export(&self) -> Result<Vec<u64>> {
        Ok(self.iter().copied().map(u64::from).collect())
    }
    fn import(p: &Vec<u64>) -> Result<Self> {
        p.iter()
            .map(|&i| {
                u32::try_from(i).map_err(|_| Error::Parse("bitmap index out of range".into()))
            })
            .collect()
    }
}

macro_rules! enumeration {
    ($name:ident { $($variant:ident = $value:literal,)* }) => {
        impl Scalar<i32> for n::$name {
            fn export(&self) -> Result<i32> {
                match *self { $(Self::$variant => Ok($value),)* Self(value) if value == 0 => Ok(0), _ => Err(Error::Parse(stringify!($name).into())) }
            }
            fn import(value: &i32) -> Result<Self> {
                match *value { $($value => Ok(Self::$variant),)* 0 => Ok(Self(0)), _ => Err(Error::Parse(stringify!($name).into())) }
            }
        }
    };
}

macro_rules! export {
    ($v:expr, $d:ident, scalar) => {
        $v.export()?
    };
    ($v:expr, $d:ident, wrapped_string) => {{
        if $v.is_some() {
            $d.child()?;
        }
        $v.as_ref().map(|s| pb::String { sval: s.clone() })
    }};
    ($v:expr, $d:ident, nodes) => {
        $v.iter()
            .map(|node| node.to_proto($d))
            .collect::<Result<_>>()?
    };
    ($v:expr, $d:ident, child($ty:ident)) => {
        $v.as_deref()
            .map(|v| v.to_proto($d).map(Into::into))
            .transpose()?
    };
    ($v:expr, $d:ident, embedded) => {
        Some($v.to_proto($d)?.into())
    };
}
macro_rules! import {
    ($v:expr, $d:ident, scalar) => {
        Scalar::import(&$v)?
    };
    ($v:expr, $d:ident, wrapped_string) => {{
        if $v.is_some() {
            $d.child()?;
        }
        $v.as_ref().map(|s| s.sval.clone())
    }};
    ($v:expr, $d:ident, nodes) => {
        $v.iter()
            .map(|node| n::Node::from_proto(node, $d))
            .collect::<Result<_>>()?
    };
    ($v:expr, $d:ident, child($ty:ident)) => {
        $v.as_ref()
            .map(|v| n::$ty::from_proto(v, $d).map(|n| Box::new(n).into()))
            .transpose()?
    };
    ($v:expr, $d:ident, embedded) => {
        $v.as_ref()
            .map(|v| n::CreateStmt::from_proto(v, $d))
            .transpose()?
            .unwrap_or_default()
    };
}

macro_rules! write_field {
    ($p:ident, $v:ident, $d:ident, $field:ident => omitted) => {
        if !is_default(&$v.$field) { return Err(Error::Parse(concat!("unsupported raw field ", stringify!($field)).into())); }
    };
    ($p:ident, $v:ident, $d:ident, $field:ident => $wire:ident $kind:ident $(($ty:ident))?) => {
        $p.$wire = export!($v.$field, $d, $kind $(($ty))?);
    };
}
macro_rules! read_field {
    ($v:ident, $p:ident, $d:ident, $field:ident => omitted) => {};
    ($v:ident, $p:ident, $d:ident, $field:ident => $wire:ident $kind:ident $(($ty:ident))?) => {
        $v.$field = import!($p.$wire, $d, $kind $(($ty))?);
    };
}

macro_rules! record {
    ($name:ident => $proto:ident { $($field:ident => $wire:ident $($kind:ident $(($ty:ident))?)?,)* } [$($extra:ident),*]) => {
        impl Record for n::$name {
            type Proto = pb::$proto;
            #[allow(unused_variables, unused_mut)]
            fn to_proto(&self, depth: Depth) -> Result<Self::Proto> {
                let depth = depth.child()?;
                let mut msg = pb::$proto::default();
                $(write_field!(msg, self, depth, $field => $wire $($kind $(($ty))?)?);)*
                Ok(msg)
            }
            #[allow(unused_variables, unused_mut)]
            fn from_proto(msg: &Self::Proto, depth: Depth) -> Result<Self> {
                let depth = depth.child()?;
                $(if !is_default(&msg.$extra) { return Err(Error::Parse(concat!("unsupported raw field ", stringify!($extra)).into())); })*
                let mut n = Self::default();
                $(read_field!(n, msg, depth, $field => $wire $($kind $(($ty))?)?);)*
                Ok(n)
            }
        }
    };
}

fn integer(i: i32) -> pb::Node {
    pb::Node {
        node: Some(NodeEnum::Integer(pb::Integer { ival: i })),
    }
}
fn integer_items(nodes: &[pb::Node]) -> impl Iterator<Item = Result<i32>> + '_ {
    nodes.iter().map(|n| match &n.node {
        Some(NodeEnum::Integer(i)) => Ok(i.ival),
        _ => Err(Error::Parse("non-integer in numeric list".into())),
    })
}

// Keep each conversion in its own frame. A single unoptimized dispatch frame
// containing temporaries for every PG node can exceed 100 KiB per child.
trait Tagged: Record {
    fn wrap(&self, depth: Depth) -> Result<pb::Node>;
    fn unwrap(msg: &Self::Proto, depth: Depth) -> Result<n::Node>;
}

macro_rules! node_records {
    ($($name:ident($variant:ident),)*) => {
        $(impl Tagged for n::$name {
            fn wrap(&self, depth: Depth) -> Result<pb::Node> {
                Ok(pb::Node { node: Some(NodeEnum::$variant(self.to_proto(depth)?.into())) })
            }
            fn unwrap(msg: &Self::Proto, depth: Depth) -> Result<n::Node> {
                Ok(Self::from_proto(msg, depth)?.into())
            }
        })*
        impl Record for n::Node {
            type Proto = pb::Node;
            fn to_proto(&self, depth: Depth) -> Result<pb::Node> {
                let d = depth.child()?;
                match self {
                    Self::Null => Ok(pb::Node::default()),
                    $(Self::$name(v) => v.wrap(d),)*
                    other => special_to_proto(other, d.child()?),
                }
            }
            fn from_proto(p: &pb::Node, depth: Depth) -> Result<Self> {
                let d = depth.child()?;
                match &p.node {
                    None => Ok(Self::Null),
                    Some(node) => native_node(node.to_ref(), d),
                }
            }
        }
        fn native_node(node: NodeRef<'_>, depth: Depth) -> Result<n::Node> {
            match node {
                $(NodeRef::$variant(v) => n::$name::unwrap(v, depth),)*
                other => special_from_proto(other, depth.child()?),
            }
        }
    };
}

// A numeric list wraps each scalar in both a Node and an Integer message.
fn numeric_list(depth: Depth, len: usize) -> Result<()> {
    if len != 0 {
        depth.child()?.child()?;
    }
    Ok(())
}

fn special_to_proto(node: &n::Node, depth: Depth) -> Result<pb::Node> {
    use n::Node::*;
    let node = match node {
        AConst(v) => NodeEnum::AConst(v.to_proto(depth)?),
        Integer(v) => return Ok(integer(*v)),
        Boolean(v) => NodeEnum::Boolean(pb::Boolean { boolval: *v }),
        String(s) => NodeEnum::String(pb::String { sval: s.clone() }),
        Float(s) => NodeEnum::Float(pb::Float { fval: s.clone() }),
        BitString(s) => NodeEnum::BitString(pb::BitString { bsval: s.clone() }),
        List(v) => NodeEnum::List(pb::List {
            items: export!(v, depth, nodes),
        }),
        IntList(v) => {
            numeric_list(depth, v.len())?;
            NodeEnum::IntList(pb::IntList {
                items: v.iter().copied().map(integer).collect(),
            })
        }
        OidList(v) => {
            numeric_list(depth, v.len())?;
            NodeEnum::OidList(pb::OidList {
                items: v.iter().map(|&i| integer(i as i32)).collect(),
            })
        }
        other => return Err(Error::Parse(other.tag().unwrap().pg_name().into())),
    };
    Ok(pb::Node { node: Some(node) })
}

fn special_from_proto(node: NodeRef<'_>, depth: Depth) -> Result<n::Node> {
    Ok(match node {
        NodeRef::AConst(v) => n::Node::AConst(n::AConst::from_proto(v, depth)?),
        NodeRef::Integer(v) => n::Node::Integer(v.ival),
        NodeRef::Boolean(v) => n::Node::Boolean(v.boolval),
        NodeRef::String(v) => n::Node::String(v.sval.clone()),
        NodeRef::Float(v) => n::Node::Float(v.fval.clone()),
        NodeRef::BitString(v) => n::Node::BitString(v.bsval.clone()),
        NodeRef::List(v) => n::Node::from(
            v.items
                .iter()
                .map(|n| n::Node::from_proto(n, depth))
                .collect::<Result<Vec<_>>>()?,
        ),
        NodeRef::IntList(v) => {
            numeric_list(depth, v.items.len())?;
            n::Node::IntList(integer_items(&v.items).collect::<Result<_>>()?)
        }
        NodeRef::OidList(v) => {
            numeric_list(depth, v.items.len())?;
            n::Node::OidList(
                integer_items(&v.items)
                    .map(|v| v.map(|i| i as u32))
                    .collect::<Result<_>>()?,
            )
        }
        other => return Err(Error::Parse(other.kind_name().into())),
    })
}

impl Record for n::AConst {
    type Proto = pb::AConst;
    fn to_proto(&self, depth: Depth) -> Result<Self::Proto> {
        use pb::a_const::Val;
        if !matches!(self.val, n::Literal::Null) {
            depth.child()?;
        }
        let val = match &self.val {
            n::Literal::Null => None,
            n::Literal::Integer(i) => Some(Val::Ival(pb::Integer { ival: *i })),
            n::Literal::Boolean(b) => Some(Val::Boolval(pb::Boolean { boolval: *b })),
            n::Literal::Float(s) => Some(Val::Fval(pb::Float { fval: s.clone() })),
            n::Literal::String(s) => Some(Val::Sval(pb::String { sval: s.clone() })),
            n::Literal::BitString(s) => Some(Val::Bsval(pb::BitString { bsval: s.clone() })),
        };
        Ok(pb::AConst {
            isnull: val.is_none(),
            val,
            location: self.location.as_pg(),
        })
    }
    fn from_proto(p: &Self::Proto, depth: Depth) -> Result<Self> {
        use pb::a_const::Val;
        if p.val.is_some() {
            depth.child()?;
        }
        let val = match (&p.val, p.isnull) {
            (None, true) => n::Literal::Null,
            (Some(Val::Ival(i)), false) => n::Literal::Integer(i.ival),
            (Some(Val::Boolval(b)), false) => n::Literal::Boolean(b.boolval),
            (Some(Val::Fval(s)), false) => n::Literal::Float(s.fval.clone()),
            (Some(Val::Sval(s)), false) => n::Literal::String(s.sval.clone()),
            (Some(Val::Bsval(s)), false) => n::Literal::BitString(s.bsval.clone()),
            _ => return Err(Error::Parse("A_Const requires a value or isnull".into())),
        };
        Ok(Self {
            val,
            location: n::Location::import(&p.location)?,
        })
    }
}
include!(concat!(env!("OUT_DIR"), "/bridge.rs"));
