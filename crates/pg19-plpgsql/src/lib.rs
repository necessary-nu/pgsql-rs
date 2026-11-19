//! PostgreSQL 19 PL/pgSQL body grammar. See src/postgres/COPYRIGHT.
#![forbid(unsafe_code)]
mod body;
mod catalog;
pub use catalog::{Builtin, NativeSql, BUILTINS};
mod compile;
mod driver;
pub mod json;
mod scope;
pub use body::*;
pub use compile::compile_block;
mod scanner;
pub use driver::*;
pub use pg19_engine::ParseMode;
pub use pg19_lexer::{Error, Options, Value};
pub use pg19_tables::plpgsql::{token, Production};
pub use scanner::*;
mod routines;
mod signature;
pub use routines::{compile_routines, parse_routines, Routine, RoutineError};
