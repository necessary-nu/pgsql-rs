//! PostgreSQL 19 PL/pgSQL body grammar. See src/postgres/COPYRIGHT.
#![forbid(unsafe_code)]
mod body;
mod compile;
mod driver;
mod scope;
pub use body::*;
pub use compile::compile_block;
mod scanner;
pub use driver::*;
pub use pg19_engine::ParseMode;
pub use pg19_lexer::{Error, Options, Value};
pub use pg19_tables::plpgsql::{token, Production};
pub use scanner::*;
