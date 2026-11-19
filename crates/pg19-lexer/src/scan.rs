use crate::{
    dfa::{longest, State},
    truncate_identifier, Error, Kind, Options, Token, Value,
};

pub struct Scanner<'a> {
    pub(crate) sql: &'a str,
    cursor: usize,
    pub(crate) options: Options,
}

impl<'a> Scanner<'a> {
    pub fn new(sql: &'a str) -> Self {
        Self::with_options(sql, Options::default())
    }

    pub fn with_options(sql: &'a str, options: Options) -> Self {
        // The reference accepts a NUL-terminated input.
        Self {
            sql: sql.split('\0').next().unwrap_or(""),
            cursor: 0,
            options,
        }
    }

    /// Byte offset after the last consumed token or whitespace. At end of
    /// input this points to the terminating NUL or the end of the string.
    pub fn offset(&self) -> usize {
        self.cursor
    }

    pub fn grammar(self) -> crate::GrammarScanner<'a> {
        crate::GrammarScanner::new(self)
    }

    fn one(&mut self) -> Result<Option<Token>, Error> {
        while self.cursor < self.sql.len() {
            let start = self.cursor;
            let (action, mut end) = longest(State::Initial, self.sql.as_bytes(), start);
            let text = &self.sql[start..end];
            let mut keyword = None;
            let (kind, value) = match action {
                1 => {
                    self.cursor = end;
                    continue;
                }
                2 => (Kind::SQL_COMMENT, Value::None),
                3 => {
                    end = block_comment(self.sql, start)?;
                    (Kind::C_COMMENT, Value::None)
                }
                9 | 12 | 14..=16 | 39 | 40 => {
                    let (kind, literal, next) = self.quoted(action, start, end)?;
                    end = next;
                    (kind, Value::Text(literal))
                }
                13 | 45 | 71 => {
                    let word = if action == 13 {
                        end = start + 1;
                        "nchar".to_owned()
                    } else if action == 45 {
                        end = start + 1;
                        "u".to_owned()
                    } else {
                        text.to_ascii_lowercase()
                    };
                    let (k, v, category) = identifier(word);
                    keyword = category;
                    (k, v)
                }
                33 => {
                    let next = self.sql[end..].find(text).ok_or_else(|| {
                        Error::near(
                            self.sql,
                            start,
                            self.sql.len(),
                            "unterminated dollar-quoted string",
                        )
                    })?;
                    let value = self.sql[end..end + next].to_owned();
                    end += next + text.len();
                    (Kind::SCONST, Value::Text(value))
                }
                34 => {
                    end = start + 1;
                    (Kind(b'$'.into()), Value::None)
                }
                46..=54 | 72 => (operator_kind(text), Value::None),
                55 => {
                    let operator = trim_operator(text);
                    end = start + operator.len();
                    if operator.len() >= 64 {
                        return Err(Error::near(self.sql, start, end, "operator too long"));
                    }
                    let kind = operator_kind(operator);
                    let value = if kind == Kind::OP {
                        Value::Text(operator.to_owned())
                    } else {
                        Value::None
                    };
                    (kind, value)
                }
                56 => {
                    let number = text[1..].parse::<i32>().map_err(|_| {
                        Error::near(self.sql, start, end, "parameter number too large")
                    })?;
                    (Kind::PARAM, Value::Integer(number))
                }
                57..=60 | 65 => {
                    if action == 65 {
                        end -= 2;
                    }
                    integer(&self.sql[start..end], action)
                }
                61..=63 | 67..=70 => {
                    let message = match action {
                        61 => "invalid hexadecimal integer",
                        62 => "invalid octal integer",
                        63 => "invalid binary integer",
                        _ => "trailing junk after numeric literal",
                    };
                    return Err(Error::near(self.sql, start, end, message));
                }
                64 | 66 => (Kind::FCONST, Value::Text(text.to_owned())),
                _ => unreachable!("unmapped initial scanner action {action}"),
            };
            self.cursor = end;
            return Ok(Some(Token {
                kind,
                start,
                end,
                value,
                keyword,
            }));
        }
        Ok(None)
    }
}

impl Iterator for Scanner<'_> {
    type Item = Result<Token, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        match self.one() {
            Ok(token) => token.map(Ok),
            Err(error) => {
                self.cursor = self.sql.len();
                Some(Err(error))
            }
        }
    }
}

// [spec:pgq:req:lex.comments]
fn block_comment(sql: &str, start: usize) -> Result<usize, Error> {
    let mut cursor = start + 2;
    let mut depth = 1;
    while cursor < sql.len() {
        let (action, end) = longest(State::Comment, sql.as_bytes(), cursor);
        if action == 4 {
            depth += 1;
            cursor += 2;
            continue;
        }
        if action == 5 {
            depth -= 1;
            if depth == 0 {
                return Ok(end);
            }
        }
        cursor = end;
    }
    Err(Error::near(
        sql,
        start,
        sql.len(),
        "unterminated /* comment",
    ))
}

// [spec:pgq:req:lex.keywords]
fn identifier(word: String) -> (Kind, Value, Option<crate::KeywordKind>) {
    match crate::keyword(&word) {
        Some((word, kind, category)) => (kind, Value::Keyword(word), Some(category)),
        None => (Kind::IDENT, Value::Text(truncate_identifier(word)), None),
    }
}

// [spec:pgq:req:lex.numbers-and-parameters]
fn integer(text: &str, action: u8) -> (Kind, Value) {
    let (radix, digits) = match action {
        58 => (16, &text[2..]),
        59 => (8, &text[2..]),
        60 => (2, &text[2..]),
        _ => (10, text),
    };
    match i32::from_str_radix(&digits.replace('_', ""), radix) {
        Ok(value) => (Kind::ICONST, Value::Integer(value)),
        Err(_) => (Kind::FCONST, Value::Text(text.to_owned())),
    }
}

fn trim_operator(text: &str) -> &str {
    let end = [text.find("/*"), text.find("--")]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(text.len());
    let op = &text[..end];
    if op.bytes().any(|byte| b"~!@#^&|`?%".contains(&byte)) {
        return op;
    }
    let trimmed = op.trim_end_matches(['+', '-']);
    &op[..trimmed.len().max(1)]
}

fn operator_kind(text: &str) -> Kind {
    match text {
        "::" => Kind::TYPECAST,
        ".." => Kind::DOT_DOT,
        ":=" => Kind::COLON_EQUALS,
        "=>" => Kind::EQUALS_GREATER,
        "<=" => Kind::LESS_EQUALS,
        ">=" => Kind::GREATER_EQUALS,
        "<>" | "!=" => Kind::NOT_EQUALS,
        _ if text.len() == 1 && !b"~!@#&|`?".contains(&text.as_bytes()[0]) => {
            Kind(text.as_bytes()[0].into())
        }
        _ => Kind::OP,
    }
}
