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

/// Look up an already lowercased word in the pinned PostgreSQL keyword catalog.
pub fn keyword(word: &str) -> Option<(&'static str, Kind, KeywordKind)> {
    keyword_case_insensitive(word).filter(|row| row.0 == word)
}

fn keyword_case_insensitive(word: &str) -> Option<(&'static str, Kind, KeywordKind)> {
    const TABLE: &[u8] = include_bytes!("../data/keyword-buckets.bin");
    #[inline]
    fn entry(index: usize) -> usize {
        usize::from(u16::from_le_bytes([TABLE[2 * index], TABLE[2 * index + 1]]))
    }
    let bytes = word.as_bytes();
    if bytes.is_empty() || bytes.len() > catalog::MAX_KEYWORD_LEN {
        return None;
    }
    let row = entry(bytes.len() * 32 + usize::from(bytes[0] & 31));
    let bucket = entry(catalog::KEYWORD_ROWS + row * 32 + usize::from(bytes[bytes.len() - 1] & 31));
    let start = catalog::KEYWORD_CANDIDATES + (bucket >> 3);
    (start..start + (bucket & 7))
        .map(|i| catalog::KEYWORDS[entry(i)])
        .find(|row| row.0.eq_ignore_ascii_case(word))
}

#[cfg(test)]
mod keyword_tests {
    use super::*;

    #[test]
    fn buckets_verify_every_keyword_and_reject_collisions() {
        for &row in catalog::KEYWORDS {
            assert_eq!(keyword(row.0), Some(row));
            let upper = row.0.to_ascii_uppercase();
            assert_eq!(keyword_case_insensitive(&upper), Some(row));
            assert_eq!(keyword(&upper), None);
            if row.0.len() > 2 {
                let mut collision = upper;
                collision.replace_range(1..2, "!");
                assert_eq!(keyword_case_insensitive(&collision), None);
            }
        }
    }
}

impl Kind {
    pub fn from_id(id: u16) -> Option<Self> {
        (id < 128 || (id >= 256 && usize::from(id - 256) < catalog::NAMES.len()))
            .then_some(Self(id))
    }

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

impl Default for Token {
    fn default() -> Self {
        Self {
            kind: Kind(0),
            start: 0,
            end: 0,
            keyword: None,
            value: Value::None,
        }
    }
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

// [spec:pgq:req:grammar.diagnostics]
impl Error {
    pub fn cursor_position(&self) -> Option<std::num::NonZeroUsize> {
        std::num::NonZeroUsize::new(self.position)
    }

    pub fn without_position(message: impl Into<String>, sqlstate: &'static str) -> Self {
        Self {
            message: message.into(),
            sqlstate,
            position: 0,
        }
    }

    /// PostgreSQL stores byte offsets in nodes but reports one-based character positions.
    pub fn at(
        sql: &str,
        offset: usize,
        message: impl Into<String>,
        sqlstate: &'static str,
    ) -> Self {
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

    /// The byte range is the parser's current lexeme, or an empty range at EOF.
    pub fn near(sql: &str, start: usize, end: usize, message: &str) -> Self {
        let message = if start == sql.len() {
            format!("{message} at end of input")
        } else {
            let text = String::from_utf8_lossy(&sql.as_bytes()[start..end]);
            format!("{message} at or near \"{text}\"")
        };
        Self::at(sql, start, message, "42601")
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BackslashQuote {
    Off,
    On,
    #[default]
    SafeEncoding,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// PostgreSQL's backslash_quote = off. The UTF-8 default is safe_encoding.
    pub disable_backslash_quote: bool,
}

// [spec:pgq:req:grammar.string-settings]
impl Options {
    /// Both on and safe_encoding allow escaped quotes for the scanner's UTF-8 input.
    pub fn with_backslash_quote(setting: BackslashQuote) -> Self {
        Self {
            disable_backslash_quote: setting == BackslashQuote::Off,
        }
    }
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
