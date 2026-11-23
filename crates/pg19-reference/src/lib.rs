//! Comparison helpers for the PostgreSQL 19 reference corpus.
#![forbid(unsafe_code)]

mod tree;
pub use tree::{node_text, nodes_text, statements_text, TreeError};
