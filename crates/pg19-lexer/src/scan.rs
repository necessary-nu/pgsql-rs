use crate::{
    dfa::{longest, State},
    Error, Kind, Options, Token, TokenText, Value, MAX_IDENTIFIER_BYTES,
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

    /// SQL visible to the scanner, ending at PostgreSQL's NUL terminator.
    pub fn source(&self) -> &'a str {
        self.sql
    }

    pub fn grammar(self) -> crate::GrammarScanner<'a> {
        crate::GrammarScanner::new(self)
    }

    pub fn grammar_with_text<T: TokenText>(self) -> crate::GrammarScanner<'a, T> {
        crate::GrammarScanner::new(self)
    }

    pub(crate) fn one<T: TokenText>(&mut self) -> Result<Option<Token<T>>, Error> {
        let mut token = Token::default();
        Ok(self.read_into(&mut token)?.then_some(token))
    }

    #[inline]
    pub(crate) fn read_into<T: TokenText>(&mut self, token: &mut Token<T>) -> Result<bool, Error> {
        let bytes = self.sql.as_bytes();
        let mut start = self.cursor;
        while matches!(
            bytes.get(start),
            Some(b' ' | b'\t' | b'\n' | b'\r' | 11 | 12)
        ) {
            start += 1;
        }
        self.cursor = start;
        let input = &bytes[start..];
        let Some(&first) = input.first() else {
            return Ok(false);
        };
        if let Some(len) = crate::dfa::ordinary_word(input) {
            self.cursor = start + len;
            identifier(
                token,
                &self.sql[start..self.cursor],
                input,
                start,
                self.cursor,
            );
            return Ok(true);
        }
        let (len, kind, value) = if matches!(first, b'(' | b')' | b',' | b';' | b'[' | b']') {
            (1, Kind(u16::from(first)), Value::None)
        } else if first.is_ascii_digit() {
            let mut len = 0;
            let mut integer = 0;
            while len < 9 && input.get(len).is_some_and(u8::is_ascii_digit) {
                integer = 10 * integer + i32::from(input[len] - b'0');
                len += 1;
            }
            // A delimiter or EOF terminates a decimal integer. Leave possible
            // prefixes, float exponents, overflow and trailing junk to Flex.
            if !matches!(
                input.get(len),
                None | Some(b' ' | b'\t' | b'\r' | b'\n' | b')' | b']' | b',' | b';')
            ) {
                return self.read_slow(token);
            }
            (len, Kind::ICONST, Value::Integer(integer))
        } else {
            return self.read_slow(token);
        };
        self.cursor = start + len;
        *token = Token {
            kind,
            value,
            keyword: None,
            start,
            end: self.cursor,
        };
        Ok(true)
    }

    fn read_slow<T: TokenText>(&mut self, token: &mut Token<T>) -> Result<bool, Error> {
        while self.cursor < self.sql.len() {
            let start = self.cursor;
            let (action, mut end) = longest(State::Initial, self.sql.as_bytes(), start);
            let text = &self.sql[start..end];
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
                    (kind, Value::Text(literal.into()))
                }
                13 | 45 | 71 => {
                    let word = if action == 13 {
                        end = start + 1;
                        "nchar"
                    } else if action == 45 {
                        end = start + 1;
                        "u"
                    } else {
                        text
                    };
                    self.cursor = end;
                    identifier(token, word, word.as_bytes(), start, end);
                    return Ok(true);
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
                    let value = T::from(&self.sql[end..end + next]);
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
                        Value::Text(operator.into())
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
                64 | 66 => (Kind::FCONST, Value::Text(text.into())),
                _ => unreachable!("unmapped initial scanner action {action}"),
            };
            self.cursor = end;
            *token = Token {
                kind,
                start,
                end,
                value,
                keyword: None,
            };
            return Ok(true);
        }
        Ok(false)
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
fn identifier<T: TokenText>(
    token: &mut Token<T>,
    word: &str,
    input: &[u8],
    start: usize,
    end: usize,
) {
    if let Some((name, kind, category)) = crate::identifier_keyword(word, input) {
        token.kind = kind;
        token.keyword = Some(category);
        token.value = Value::Keyword(name);
    } else {
        token.kind = Kind::IDENT;
        token.keyword = None;
        let prefix = &word[..word.floor_char_boundary(MAX_IDENTIFIER_BYTES)];
        token.value = Value::Text(T::lowercase_identifier(prefix));
    }
    token.start = start;
    token.end = end;
}

// [spec:pgq:req:lex.numbers-and-parameters]
fn integer<T: TokenText>(text: &str, action: u8) -> (Kind, Value<T>) {
    let (radix, digits) = match action {
        58 => (16, &text[2..]),
        59 => (8, &text[2..]),
        60 => (2, &text[2..]),
        _ => (10, text),
    };
    let cleaned;
    let digits = if digits.as_bytes().contains(&b'_') {
        cleaned = digits.replace('_', "");
        &cleaned
    } else {
        digits
    };
    match i32::from_str_radix(digits, radix) {
        Ok(value) => (Kind::ICONST, Value::Integer(value)),
        Err(_) => (Kind::FCONST, Value::Text(text.into())),
    }
}

fn trim_operator(text: &str) -> &str {
    if text.len() == 1 {
        return text;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_tokens_follow_flex_at_every_ascii_boundary() {
        for prefix in [
            " ",
            "\t",
            "\n",
            "\r",
            "\x0b",
            "\x0c",
            "(",
            ")",
            "[",
            "]",
            ",",
            ";",
            "a",
            "U",
            "E",
            "B",
            "n",
            "x",
            "ab_9",
            "Ä",
            "0",
            "123456789",
            "1234567890",
        ] {
            for suffix in 1..=127u8 {
                let sql = format!("{prefix}{}z", char::from(suffix));
                let mut fast = Scanner::new(&sql);
                let mut flex = Scanner::new(&sql);
                let mut left = Token::<String>::default();
                let mut right = Token::default();
                loop {
                    let actual = fast.read_into(&mut left);
                    let expected = flex.read_slow(&mut right);
                    assert_eq!(actual, expected, "{sql:?}");
                    assert_eq!(fast.offset(), flex.offset(), "{sql:?}");
                    if actual != Ok(true) {
                        break;
                    }
                    assert_eq!(left, right, "{sql:?}");
                }
            }
        }
    }
}
