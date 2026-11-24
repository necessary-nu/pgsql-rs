//! Match the pinned native record and field names to the protobuf descriptor.
//! The generated invocations are type checked against both Rust representations.
use heck::{ToSnakeCase, ToUpperCamelCase};
use prost_types::{field_descriptor_proto::Type, FileDescriptorProto};
use std::fmt::Write;

macro_rules! node_types {
    (values { $($values:tt)* } records {
        $($ty:ident ($id:literal, $name:literal) { $($field:ident [$key:literal]: $kind:ty,)* })*
    }) => {
        const RECORDS: &[Record] = &[$(Record {
            native: stringify!($ty), name: $name,
            fields: &[$((stringify!($field), $key, stringify!($kind))),*],
        }),*];
    };
}
include!("../../pgsql-nodes/src/nodes.in");

struct Record {
    native: &'static str,
    name: &'static str,
    fields: &'static [(&'static str, &'static str, &'static str)],
}

fn ident(name: &str) -> String {
    // Raw identifiers work on both ordinary field names and Rust keywords.
    format!("r#{}", name.to_snake_case())
}

pub fn generate(schema: &FileDescriptorProto) -> String {
    let node = schema
        .message_type
        .iter()
        .find(|m| m.name() == "Node")
        .unwrap();
    let mut source = String::new();
    let mut variants = String::new();

    for record in RECORDS {
        let msg = schema
            .message_type
            .iter()
            .find(|m| m.name() == record.name)
            .unwrap();
        let variant = node
            .field
            .iter()
            .find(|f| f.json_name() == record.name)
            .unwrap()
            .name()
            .to_upper_camel_case();
        let pb = msg.name().to_upper_camel_case();
        writeln!(variants, "{}({variant}),", record.native).unwrap();
        writeln!(source, "record!({} => {pb} {{", record.native).unwrap();
        for &(field, key, kind) in record.fields {
            let Some(wire) = msg.field.iter().find(|f| f.json_name() == key) else {
                writeln!(source, "{field} => omitted,").unwrap();
                continue;
            };
            let kind = kind.replace(' ', "");
            let conversion = match kind.as_str() {
                "Vec<Node>" => "nodes".into(),
                "CreateStmt" => "embedded".into(),
                "Option<String>" if wire.r#type() == Type::Message => "wrapped_string".into(),
                _ if kind.starts_with("Option<") => {
                    if kind == "Option<String>" {
                        "scalar".into()
                    } else {
                        let child = kind.rsplit('<').next().unwrap().trim_end_matches('>');
                        format!("child({child})")
                    }
                }
                _ => "scalar".into(),
            };
            writeln!(source, "{field} => {} {conversion},", ident(wire.name())).unwrap();
        }
        let extra = msg
            .field
            .iter()
            .filter(|f| {
                !record
                    .fields
                    .iter()
                    .any(|(_, key, _)| *key == f.json_name())
            })
            .map(|f| ident(f.name()))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(source, "}} [{extra}]);").unwrap();
    }
    writeln!(source, "node_records! {{ {variants} }}").unwrap();
    source
}

macro_rules! pg_enums {
    ($($name:ident { $($variant:ident = $value:expr,)* })*) => {
        pub fn enums(schema: &FileDescriptorProto) -> String {
            let mut output = String::new();
            for (name, names) in [$( (stringify!($name), &[$(stringify!($variant)),*] as &[&str]) ),*] {
                let Some(desc) = schema.enum_type.iter().find(|e| e.name().to_upper_camel_case() == name.to_upper_camel_case()) else { continue; };
                writeln!(output, "enumeration!({name} {{").unwrap();
                for variant in names {
                    if let Some(v) = desc.value.iter().find(|v| v.name() == *variant) {
                        writeln!(output, "{variant} = {},", v.number()).unwrap();
                    }
                }
                writeln!(output, "}});").unwrap();
            }
            output
        }
    };
}
include!("../../pgsql-nodes/src/enums.in");
