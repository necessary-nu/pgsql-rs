use super::{fields::Wire, *};

fn set<T: Wire + Default + PartialEq>(
    m: &mut DynamicMessage,
    name: &str,
    field_value: &T,
    d: Depth,
) -> Result<()> {
    if let Some(f) = m.descriptor().get_field_by_json_name(name) {
        if let Some(v) = field_value.pack(&f, d)? {
            m.set_field(&f, v);
        }
    } else if *field_value != T::default() {
        return Err(ProtobufError::Unsupported(format!(
            "{}.{}",
            m.descriptor().name(),
            name
        )));
    }
    Ok(())
}

fn get<T: Wire>(m: &mut DynamicMessage, name: &str, field_value: &mut T, d: Depth) -> Result<()> {
    if let Some(f) = m.descriptor().get_field_by_json_name(name) {
        if let Some(v) = m.take_field(&f) {
            *field_value = T::unpack(v, &f, d)?;
        }
    }
    Ok(())
}

fn wrapped<T: Record + Into<Node>>(v: Value, d: Depth) -> Result<Node> {
    T::from_message(v, d).map(Into::into)
}

macro_rules! node_types {
    (values { $($value:ident ($value_id:literal, $value_name:literal, $payload:ty))* }
     records { $($ty:ident ($id:literal, $pg:literal) { $($field:ident [$cname:literal]: $ftype:ty,)* })* }
    ) => {
        $(impl Record for $ty {
            const NAME: &'static str = $pg;
            #[allow(unused_variables)]
            fn write(&self, m: &mut DynamicMessage, d: Depth) -> Result<()> {
                $(set(m, $cname, &self.$field, d)?;)*
                Ok(())
            }
            #[allow(unused_variables, unused_mut)]
            fn read(mut m: DynamicMessage, d: Depth) -> Result<Self> {
                let mut n = Self::default();
                $(get(&mut m, $cname, &mut n.$field, d)?;)*
                if let Some((field, _)) = m.fields().next() {
                    return Err(ProtobufError::Unsupported(field.full_name().into()));
                }
                Ok(n)
            }
        })*
        impl Record for Node {
            const NAME: &'static str = "Node";
            fn write(&self, m: &mut DynamicMessage, d: Depth) -> Result<()> {
                let Some(tag) = self.tag() else { return Ok(()); };
                let f = m.descriptor().get_field_by_json_name(tag.pg_name())
                    .ok_or_else(|| ProtobufError::Unsupported(tag.pg_name().into()))?;
                let record: Option<&dyn PackRecord> = match self {
                    $(Self::$ty(x) => Some(&**x),)*
                    Self::AConst(x) => Some(x),
                    _ => None,
                };
                let v = match record {
                    Some(record) => record.to_message(d)?,
                    None => values::pack_special(self, d)?,
                };
                m.set_field(&f, v);
                Ok(())
            }
            fn read(mut m: DynamicMessage, d: Depth) -> Result<Self> {
                let Some((f, v)) = m.take_fields().next() else {
                    if m.unknown_fields().next().is_some() { return Err(ProtobufError::Invalid("unknown node discriminant".into())); }
                    return Ok(Self::Null);
                };
                match f.json_name() {
                    $($pg => wrapped::<$ty>(v, d),)*
                    "A_Const" => wrapped::<AConst>(v, d),
                    name => values::unpack_special(name, v, d),
                }
            }
        }
    }
}
include!("../../../pgsql-nodes/src/nodes.in");
