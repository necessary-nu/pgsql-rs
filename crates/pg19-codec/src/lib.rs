//! Compatibility serialization for the native PostgreSQL 19 raw tree.
#![forbid(unsafe_code)]

mod json;
pub use json::{node_json, statements_json, EncodeError};

pub mod protobuf;
