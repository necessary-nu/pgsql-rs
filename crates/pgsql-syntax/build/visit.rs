//! Generate both borrow directions from the same PG19 descriptor fields.
use heck::{ToSnakeCase, ToUpperCamelCase};
use prost_types::{
    field_descriptor_proto::{Label, Type},
    DescriptorProto, FileDescriptorProto,
};
use std::fmt::Write;

pub fn generate(file: &FileDescriptorProto, node: &DescriptorProto) -> String {
    let mut code = String::new();
    for mutable in [false, true] {
        let (view, borrow, wrap) = if mutable {
            ("NodeRefMut", "mut", "borrow_mut")
        } else {
            ("NodeRef", "ref", "to_ref")
        };
        writeln!(
            code,
            "impl<'a> {view}<'a> {{ fn children(self, stack: &mut Vec<Self>) {{ match self {{"
        )
        .unwrap();
        for variant in &node.field {
            let record = file
                .message_type
                .iter()
                .find(|r| r.name() == variant.json_name())
                .unwrap();
            let kind = variant.name().to_upper_camel_case();
            let fields: Vec<_> = record
                .field
                .iter()
                .filter(|f| f.r#type() == Type::Message)
                .collect();
            if fields.is_empty() {
                writeln!(code, "Self::{kind}(_) => {{}},").unwrap();
                continue;
            }
            writeln!(code, "Self::{kind}(record) => {{").unwrap();
            for (index, group) in record.oneof_decl.iter().enumerate() {
                let group_name = group.name().to_snake_case();
                writeln!(code, "match record.r#{group_name}.as_{borrow}() {{").unwrap();
                for field in fields
                    .iter()
                    .filter(|f| f.oneof_index == Some(index as i32))
                {
                    let target = field.type_name().rsplit('.').next().unwrap();
                    let v = node.field.iter().find(|v| v.json_name() == target).unwrap();
                    writeln!(
                        code,
                        "Some(protobuf::{}::{}::{}(item)) => stack.push(Self::{}(item)),",
                        record.name().to_snake_case(),
                        group.name().to_upper_camel_case(),
                        field.name().to_upper_camel_case(),
                        v.name().to_upper_camel_case()
                    )
                    .unwrap();
                }
                code.push_str("None => {} }\n");
            }
            for field in fields.into_iter().filter(|f| f.oneof_index.is_none()) {
                let name = field.name().to_snake_case();
                let target = field.type_name().rsplit('.').next().unwrap();
                let push = if target == "Node" {
                    format!("if let Some(node) = item.node.as_{borrow}() {{ stack.push(node.{wrap}()); }}")
                } else {
                    let v = node
                        .field
                        .iter()
                        .find(|v| v.json_name() == target)
                        .expect("node record for descriptor edge");
                    format!(
                        "stack.push(Self::{}(item));",
                        v.name().to_upper_camel_case()
                    )
                };
                if field.label() == Label::Repeated {
                    let iter = if mutable { "iter_mut" } else { "iter" };
                    writeln!(code, "for item in record.r#{name}.{iter}() {{ {push} }}").unwrap();
                } else {
                    writeln!(
                        code,
                        "if let Some(item) = record.r#{name}.as_{borrow}() {{ {push} }}"
                    )
                    .unwrap();
                }
            }
            code.push_str("},\n");
        }
        code.push_str("} } }\n");
    }
    code
}
