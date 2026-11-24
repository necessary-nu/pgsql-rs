#[path = "build/bridge.rs"]
mod bridge;
#[path = "build/visit.rs"]
mod visit;
use std::{env, fs, path::PathBuf};

use heck::ToUpperCamelCase;
use prost::Message;
use prost_types::FileDescriptorSet;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = "../../protobuf/pg19/pg_query.pb";
    println!("cargo:rerun-if-changed={path}");
    let schema = FileDescriptorSet::decode(&*fs::read(path)?)?;
    let node = schema
        .file
        .iter()
        .flat_map(|f| &f.message_type)
        .find(|m| m.name() == "Node")
        .expect("PG19 Node message");
    let mut views = String::from("node_views! {\n");
    for field in &node.field {
        let variant = field.name().to_upper_camel_case();
        let record = field
            .type_name()
            .rsplit('.')
            .next()
            .unwrap()
            .to_upper_camel_case();
        views.push_str(&format!(
            "    {variant}({record}, {:?}),\n",
            field.json_name()
        ));
    }
    views.push_str("}\n");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(out.join("node_views.rs"), views)?;
    println!("cargo:rerun-if-changed=../pgsql-nodes/src/nodes.in");
    println!("cargo:rerun-if-changed=../pgsql-nodes/src/enums.in");
    println!("cargo:rerun-if-changed=build/bridge.rs");
    let file = &schema.file[0];
    println!("cargo:rerun-if-changed=build/visit.rs");
    fs::write(out.join("visit.rs"), visit::generate(file, node))?;
    fs::write(
        out.join("bridge.rs"),
        bridge::generate(file) + &bridge::enums(file),
    )?;
    prost_build::Config::new()
        .type_attribute(".", "#[derive(serde::Serialize)]")
        .compile_fds(schema)?;
    Ok(())
}
