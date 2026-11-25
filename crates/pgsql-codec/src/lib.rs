//! Compatibility serialization for the native PostgreSQL 19 raw tree.
#![forbid(unsafe_code)]

use pgsql_nodes::*;
use std::string::String;
mod json;
mod json_view;
pub use json::EncodeError;
pub fn node_json(node: &Node) -> Result<String, EncodeError> {
    json::node_json_in(node, &json_view::Context)
}
pub fn statements_json(stmts: &[RawStmt]) -> Result<String, EncodeError> {
    json::statements_json_in(stmts, &json_view::Context)
}

pub mod protobuf;
