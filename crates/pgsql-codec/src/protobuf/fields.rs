use super::*;
use std::collections::BTreeSet;

pub(super) trait Wire: Sized {
    fn pack(&self, f: &FieldDescriptor, depth: Depth) -> Result<Option<Value>>;
    fn unpack(v: Value, f: &FieldDescriptor, depth: Depth) -> Result<Self>;
}

macro_rules! scalar {
    ($($ty:ty => $kind:ident),*) => { $(impl Wire for $ty {
        fn pack(&self, _: &FieldDescriptor, _: Depth) -> Result<Option<Value>> {
            Ok(Some(Value::$kind(*self)))
        }
        fn unpack(v: Value, _: &FieldDescriptor, _: Depth) -> Result<Self> {
            if let Value::$kind(value) = v { Ok(value) } else { Err(ProtobufError::Invalid(stringify!($ty).into())) }
        }
    })* }
}
scalar!(bool => Bool, i32 => I32, i64 => I64, u32 => U32, u64 => U64, f64 => F64);

impl Wire for Location {
    fn pack(&self, _: &FieldDescriptor, _: Depth) -> Result<Option<Value>> {
        Ok(Some(Value::I32(self.as_pg())))
    }
    fn unpack(v: Value, _: &FieldDescriptor, _: Depth) -> Result<Self> {
        v.as_i32()
            .and_then(Location::from_pg)
            .ok_or_else(|| ProtobufError::Invalid("invalid byte location".into()))
    }
}

impl Wire for u8 {
    fn pack(&self, _: &FieldDescriptor, _: Depth) -> Result<Option<Value>> {
        Ok((*self != 0).then(|| Value::String(char::from(*self).to_string())))
    }
    fn unpack(v: Value, _: &FieldDescriptor, _: Depth) -> Result<Self> {
        let s = v
            .as_str()
            .ok_or_else(|| ProtobufError::Invalid("expected character".into()))?;
        let mut chars = s.chars();
        let c = chars.next().unwrap_or('\0');
        u8::try_from(u32::from(c))
            .ok()
            .filter(|_| chars.next().is_none())
            .ok_or_else(|| ProtobufError::Invalid("expected one byte character".into()))
    }
}

impl Wire for i16 {
    fn pack(&self, _: &FieldDescriptor, _: Depth) -> Result<Option<Value>> {
        Ok(Some(Value::I32(i32::from(*self))))
    }
    fn unpack(v: Value, _: &FieldDescriptor, _: Depth) -> Result<Self> {
        v.as_i32()
            .and_then(|x| x.try_into().ok())
            .ok_or_else(|| ProtobufError::Invalid("int16 field out of range".into()))
    }
}

impl Wire for Option<String> {
    fn pack(&self, f: &FieldDescriptor, _: Depth) -> Result<Option<Value>> {
        Ok(self.as_ref().map(|s| {
            if let prost_reflect::Kind::Message(desc) = f.kind() {
                let mut m = DynamicMessage::new(desc);
                m.set_field_by_number(1, Value::String(s.clone()));
                Value::Message(m)
            } else {
                Value::String(s.clone())
            }
        }))
    }
    fn unpack(v: Value, _: &FieldDescriptor, _: Depth) -> Result<Self> {
        let value = if let Value::Message(m) = v {
            field(&m, 1)
        } else {
            v
        };
        match value {
            Value::String(s) => Ok(Some(s)),
            _ => Err(ProtobufError::Invalid("expected string".into())),
        }
    }
}

trait ChildPointer: std::ops::Deref {}
impl<T> ChildPointer for Box<T> {}
impl ChildPointer for std::sync::Arc<Node> {}
impl<T: NodeRecord> ChildPointer for NodeBox<T> {}

impl<P> Wire for Option<P>
where
    P: ChildPointer + From<Box<P::Target>>,
    P::Target: Record,
{
    fn pack(&self, _: &FieldDescriptor, d: Depth) -> Result<Option<Value>> {
        self.as_deref().map(|x| x.to_message(d)).transpose()
    }
    fn unpack(v: Value, _: &FieldDescriptor, d: Depth) -> Result<Self> {
        Ok(Some(Box::new(P::Target::from_message(v, d)?).into()))
    }
}

impl Wire for CreateStmt {
    fn pack(&self, _: &FieldDescriptor, d: Depth) -> Result<Option<Value>> {
        self.to_message(d).map(Some)
    }
    fn unpack(v: Value, _: &FieldDescriptor, d: Depth) -> Result<Self> {
        Self::from_message(v, d)
    }
}

impl Wire for Vec<Node> {
    fn pack(&self, _: &FieldDescriptor, d: Depth) -> Result<Option<Value>> {
        Ok(Some(Value::List(
            self.iter()
                .map(|x| x.to_message(d))
                .collect::<Result<_>>()?,
        )))
    }
    fn unpack(v: Value, _: &FieldDescriptor, d: Depth) -> Result<Self> {
        list(v)?
            .into_iter()
            .map(|x| Node::from_message(x, d))
            .collect()
    }
}

impl Wire for BTreeSet<u32> {
    fn pack(&self, _: &FieldDescriptor, _: Depth) -> Result<Option<Value>> {
        Ok(Some(Value::List(
            self.iter().map(|&x| Value::U64(x.into())).collect(),
        )))
    }
    fn unpack(v: Value, _: &FieldDescriptor, _: Depth) -> Result<Self> {
        list(v)?
            .into_iter()
            .map(|x| {
                x.as_u64()
                    .and_then(|n| n.try_into().ok())
                    .ok_or_else(|| ProtobufError::Invalid("bitmap index out of range".into()))
            })
            .collect()
    }
}

macro_rules! pg_enums {
    ($( $name:ident { $( $constant:ident = $number:expr, )* } )*) => { $(impl Wire for $name {
        fn pack(&self, f: &FieldDescriptor, _: Depth) -> Result<Option<Value>> {
            let prost_reflect::Kind::Enum(desc) = f.kind() else { return Err(ProtobufError::Invalid("expected enum".into())); };
            let n = match self.pg_name() {
                Some(name) => desc.get_value_by_name(name).ok_or_else(|| ProtobufError::Invalid(name.into()))?.number(),
                None if self.0 == 0 => return Ok(None),
                None => return Err(ProtobufError::Invalid(stringify!($name).into())),
            };
            Ok(Some(Value::EnumNumber(n)))
        }
        fn unpack(v: Value, f: &FieldDescriptor, _: Depth) -> Result<Self> {
            let prost_reflect::Kind::Enum(desc) = f.kind() else { return Err(ProtobufError::Invalid("expected enum".into())); };
            let number = v.as_enum_number().ok_or_else(|| ProtobufError::Invalid("expected enum value".into()))?;
            let found = desc.get_value(number).ok_or_else(|| ProtobufError::Invalid(stringify!($name).into()))?;
            match found.name() { $( stringify!($constant) => Ok(Self::$constant), )* _ => Err(ProtobufError::Invalid(found.name().into())) }
        }
    })* }
}
include!("../../../pgsql-nodes/src/enums.in");
