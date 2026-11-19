//! PostgreSQL 19 PL/pgSQL body grammar. See src/postgres/COPYRIGHT.
#![forbid(unsafe_code)]
mod driver;
mod scanner;
pub use driver::*;
pub use pg19_engine::ParseMode;
pub use pg19_lexer::{Error, Options, Value};
pub use pg19_tables::plpgsql::{token, Production};
pub use scanner::*;
