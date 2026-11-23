//! PostgreSQL 19 raw SQL parsing, serialization and query operations in Rust.
//!
//! ```
//! use pgsql_parser::{deparse, nodes::NodeTag, parse_raw, Options};
//! let tree = parse_raw("SELECT 1", Options::default())?;
//! assert_eq!(tree.statements().next().unwrap().tag(), Some(NodeTag::SelectStmt));
//! let statements = tree.into_owned();
//! assert_eq!(deparse::statements(&statements)?, "SELECT 1");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
#![forbid(unsafe_code)]

// [spec:pgq:req:api.native]
pub use pg19_codec as codec;
pub use pg19_lexer as lexer;
pub use pg19_nodes as nodes;
pub use pg19_operations::{
    deparse, fingerprint, is_utility_statement, normalize, normalize_utility, split_with_parser,
    split_with_scanner, summary, utility_statements,
};
pub use pg19_parser::{
    parse, parse_raw, parse_statements, EncodeError as RawJsonError, Error as ParseError, Options,
    ParseMode, RawStatement, RawTree,
};
pub use pg19_plpgsql::{parse_routines, Routine, RoutineError};
