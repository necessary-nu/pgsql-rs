use crate::{
    literal::{digits, surrogate_pair},
    truncate_identifier, Error, Kind, Scanner, Token, Value,
};

pub struct GrammarScanner<'a> {
    core: Scanner<'a>,
    buffered: Option<Token>,
    failed: bool,
}

impl<'a> GrammarScanner<'a> {
    pub(crate) fn new(core: Scanner<'a>) -> Self {
        Self {
            core,
            buffered: None,
            failed: false,
        }
    }

    fn read(&mut self) -> Result<Option<Token>, Error> {
        loop {
            let token = self.core.one()?;
            if !token.as_ref().is_some_and(|t| t.kind.is_comment()) {
                return Ok(token);
            }
        }
    }

    /// Fetch the next grammar token, including lookahead rewrites and EOF.
    #[inline]
    pub fn next_token(&mut self) -> Result<Option<Token>, Error> {
        let mut token = Token::default();
        Ok(self.next_into(&mut token)?.then_some(token))
    }

    /// Write the next grammar token to reusable storage and return false at EOF.
    #[inline]
    pub fn next_into(&mut self, token: &mut Token) -> Result<bool, Error> {
        if self.failed {
            return Ok(false);
        }
        self.read_into(token).inspect_err(|_| self.failed = true)
    }

    // [spec:pgq:req:lex.parser-lookahead]
    #[inline]
    fn read_into(&mut self, token: &mut Token) -> Result<bool, Error> {
        if let Some(buffered) = self.buffered.take() {
            *token = buffered;
        } else {
            loop {
                if !self.core.read_into(token)? {
                    return Ok(false);
                }
                if !token.kind.is_comment() {
                    break;
                }
            }
        }
        if !matches!(
            token.kind,
            Kind::FORMAT
                | Kind::NOT
                | Kind::NULLS_P
                | Kind::WITH
                | Kind::WITHOUT
                | Kind::UIDENT
                | Kind::USCONST
        ) {
            return Ok(true);
        }
        *token = self.rewrite(std::mem::take(token))?;
        Ok(true)
    }

    fn rewrite(&mut self, mut token: Token) -> Result<Token, Error> {
        self.buffered = self.read()?;
        let next = self.buffered.as_ref().map(|token| token.kind);
        token.kind = match (token.kind, next) {
            (Kind::FORMAT, Some(Kind::JSON)) => Kind::FORMAT_LA,
            (
                Kind::NOT,
                Some(Kind::BETWEEN | Kind::IN_P | Kind::LIKE | Kind::ILIKE | Kind::SIMILAR),
            ) => Kind::NOT_LA,
            (Kind::NULLS_P, Some(Kind::FIRST_P | Kind::LAST_P)) => Kind::NULLS_LA,
            (Kind::WITH, Some(Kind::TIME | Kind::ORDINALITY)) => Kind::WITH_LA,
            (Kind::WITHOUT, Some(Kind::TIME)) => Kind::WITHOUT_LA,
            (Kind::UIDENT | Kind::USCONST, _) => {
                let escape = if next == Some(Kind::UESCAPE) {
                    self.buffered.take();
                    let third = self.read()?;
                    let sql = self.core.sql;
                    let Some(third) = third else {
                        return Err(Error::near(
                            sql,
                            sql.len(),
                            sql.len(),
                            "UESCAPE must be followed by a simple string literal",
                        ));
                    };
                    if third.kind != Kind::SCONST {
                        return Err(Error::near(
                            sql,
                            third.start,
                            third.end,
                            "UESCAPE must be followed by a simple string literal",
                        ));
                    }
                    let Value::Text(value) = third.value else {
                        unreachable!()
                    };
                    let bytes = value.as_bytes();
                    if bytes.len() != 1
                        || bytes[0].is_ascii_hexdigit()
                        || b"+'\" \t\r\n\x0c\x0b".contains(&bytes[0])
                    {
                        return Err(Error::near(
                            sql,
                            third.start,
                            third.end,
                            "invalid Unicode escape character",
                        ));
                    }
                    // The grammar consumes the entire UESCAPE phrase as one lexeme for yyerror.
                    token.end = third.end;
                    bytes[0]
                } else {
                    b'\\'
                };
                let Value::Text(s) = &token.value else {
                    unreachable!()
                };
                let value = unescape(s, escape, self.core.sql, token.start)?;
                if token.kind == Kind::UIDENT {
                    token.value = Value::Text(truncate_identifier(value));
                    Kind::IDENT
                } else {
                    token.value = Value::Text(value);
                    Kind::SCONST
                }
            }
            (kind, _) => kind,
        };
        Ok(token)
    }
}

impl Iterator for GrammarScanner<'_> {
    type Item = Result<Token, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        self.next_token().transpose()
    }
}

// [spec:pgq:req:lex.unicode-escape]
fn unescape(input: &str, escape: u8, sql: &str, location: usize) -> Result<String, Error> {
    let bytes = input.as_bytes();
    let mut at = 0;
    let mut out = Vec::with_capacity(bytes.len());
    let mut high = None;
    while at < bytes.len() {
        let error = |message| Error::at(sql, location + 3 + at, message, "42601");
        if bytes[at] != escape || bytes.get(at + 1) == Some(&escape) {
            if high.is_some() {
                return Err(error("invalid Unicode surrogate pair"));
            }
            out.push(bytes[at]);
            at += if bytes[at] == escape { 2 } else { 1 };
            continue;
        }
        let (start, width) = if bytes.get(at + 1) == Some(&b'+') {
            (at + 2, 6)
        } else {
            (at + 1, 4)
        };
        let sequence = bytes
            .get(start..start + width)
            .filter(|v| v.iter().all(u8::is_ascii_hexdigit))
            .ok_or_else(|| error("invalid Unicode escape"))?;
        let mut cp = digits(sequence, 16);
        if cp == 0 || cp > 0x10ffff {
            return Err(error("invalid Unicode escape value"));
        }
        if let Some(first) = high.take() {
            if !(0xdc00..=0xdfff).contains(&cp) {
                return Err(error("invalid Unicode surrogate pair"));
            }
            cp = surrogate_pair(first, cp);
        } else if (0xdc00..=0xdfff).contains(&cp) {
            return Err(error("invalid Unicode surrogate pair"));
        }
        if (0xd800..=0xdbff).contains(&cp) {
            high = Some(cp);
        } else {
            let ch = char::from_u32(cp).expect("validated Unicode scalar");
            out.extend_from_slice(ch.encode_utf8(&mut [0; 4]).as_bytes());
        }
        at = start + width;
    }
    if high.is_some() {
        return Err(Error::at(
            sql,
            location + 3 + at,
            "invalid Unicode surrogate pair",
            "42601",
        ));
    }
    Ok(String::from_utf8(out).expect("complete UTF-8 code points"))
}
