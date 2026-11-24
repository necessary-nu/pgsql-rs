//! Compatibility serialization for the native PostgreSQL 19 raw tree.
#![forbid(unsafe_code)]

use pgsql_nodes::*;
use std::string::String;
mod json;
pub use json::{node_json, statements_json, EncodeError};

pub mod protobuf;
