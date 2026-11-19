//! UTF-8 PostgreSQL 19 scanner and parser lookahead, derived from the patched
//! scan.l and parser.c. See src/postgres/COPYRIGHT in the repository.
mod dfa;
mod grammar;
mod literal;
mod scan;

pub use grammar::GrammarScanner;
pub use scan::Scanner;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Kind(pub(crate) u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeywordKind {
    Unreserved,
    ColumnName,
    TypeFunctionName,
    Reserved,
}

mod catalog {
    use super::{KeywordKind, Kind};
    include!("../data/catalog.rs");
}

impl Kind {
    pub fn id(self) -> u16 {
        self.0
    }

    pub fn name(self) -> &'static str {
        if self.0 < 128 {
            let start = usize::from(self.0);
            return &ASCII[start..start + 1];
        }
        catalog::NAMES[usize::from(self.0 - 256)]
    }

    pub fn is_comment(self) -> bool {
        matches!(self, Self::C_COMMENT | Self::SQL_COMMENT)
    }
}

const ASCII: &str = "\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~\x7f";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    None,
    Integer(i32),
    Text(String),
    Keyword(&'static str),
}

// [spec:pgq:req:lex.byte-spans]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
    pub keyword: Option<KeywordKind>,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub message: String,
    pub sqlstate: &'static str,
    /// One-based character position. Zero means the backend supplied no cursor.
    pub position: usize,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Error {}

impl Error {
    fn at(sql: &str, offset: usize, message: impl Into<String>, sqlstate: &'static str) -> Self {
        let position = 1 + sql.as_bytes()[..offset.min(sql.len())]
            .iter()
            .filter(|byte| **byte & 0xc0 != 0x80)
            .count();
        Self {
            message: message.into(),
            sqlstate,
            position,
        }
    }

    fn near(sql: &str, start: usize, end: usize, message: &str) -> Self {
        let message = if start == sql.len() {
            format!("{message} at end of input")
        } else {
            let text = String::from_utf8_lossy(&sql.as_bytes()[start..end]);
            format!("{message} at or near \"{text}\"")
        };
        Self::at(sql, start, message, "42601")
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// PostgreSQL's backslash_quote = off. The UTF-8 default is safe_encoding.
    pub disable_backslash_quote: bool,
}

fn truncate_identifier(mut ident: String) -> String {
    if ident.len() > 63 {
        let mut boundary = 63;
        while !ident.is_char_boundary(boundary) {
            boundary -= 1;
        }
        ident.truncate(boundary);
    }
    ident
}
