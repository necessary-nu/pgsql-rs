//! PostgreSQL 19 raw SQL parsing, serialization and query operations in Rust.
//!
//! ```
//! use pg19_query::{deparse, parse_statements, Options};
//! let statements = parse_statements("SELECT 1", Options::default())?;
//! assert_eq!(deparse::statements(&statements)?, "SELECT 1");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
#![forbid(unsafe_code)]

pub use pg19_codec as codec;
pub use pg19_lexer as lexer;
pub use pg19_nodes as nodes;
pub use pg19_operations::{
    deparse, fingerprint, is_utility_statement, normalize, normalize_utility, split_with_parser,
    split_with_scanner, utility_statements,
};
pub use pg19_parser::{parse, parse_statements, Error as ParseError, Options, ParseMode};
pub use pg19_plpgsql::{parse_routines, Routine, RoutineError};
