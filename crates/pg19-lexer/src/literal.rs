use crate::{
    dfa::{longest, State},
    truncate_identifier, Error, Kind, Scanner,
};

impl Scanner<'_> {
    // [spec:pgq:req:lex.quoted-values]
    pub(crate) fn quoted(
        &self,
        open: u8,
        start: usize,
        mut at: usize,
    ) -> Result<(Kind, String, usize), Error> {
        let (mode, kind, unterminated) = match open {
            9 => (State::Bit, Kind::BCONST, "unterminated bit string literal"),
            12 => (
                State::Hex,
                Kind::XCONST,
                "unterminated hexadecimal string literal",
            ),
            14 => (State::Standard, Kind::SCONST, "unterminated quoted string"),
            15 => (State::Escape, Kind::SCONST, "unterminated quoted string"),
            16 => (
                State::UnicodeString,
                Kind::USCONST,
                "unterminated quoted string",
            ),
            39 => (
                State::Identifier,
                Kind::IDENT,
                "unterminated quoted identifier",
            ),
            40 => (
                State::UnicodeIdentifier,
                Kind::UIDENT,
                "unterminated quoted identifier",
            ),
            _ => unreachable!(),
        };
        let bytes = self.sql.as_bytes();
        let mut literal = Vec::new();
        if mode == State::Hex {
            literal.push(b'x');
        }
        if mode == State::Bit {
            literal.push(b'b');
        }
        while at < bytes.len() {
            let (action, end) = longest(mode, bytes, at);
            let matched = &bytes[at..end];
            match action {
                10 | 11 | 22 | 23 | 32 | 44 => literal.extend_from_slice(matched),
                17 => {
                    let (next, continued) = longest(State::AfterQuote, bytes, end);
                    if next == 18 {
                        at = continued;
                        continue;
                    }
                    return Ok((kind, text(literal)?, end));
                }
                21 => literal.push(b'\''),
                24 => {
                    let mut c = digits(&matched[2..], 16);
                    let mut stop = end;
                    if (0xd800..=0xdbff).contains(&c) {
                        let (action, next) = longest(State::Surrogate, bytes, end);
                        if action == 28 {
                            return Err(Error::at(
                                self.sql,
                                end,
                                "invalid Unicode escape",
                                "22025",
                            ));
                        }
                        if action != 25 {
                            return Err(Error::near(
                                self.sql,
                                end,
                                next,
                                "invalid Unicode surrogate pair",
                            ));
                        }
                        let low = digits(&bytes[end + 2..next], 16);
                        if !(0xdc00..=0xdfff).contains(&low) {
                            return Err(Error::near(
                                self.sql,
                                end,
                                next,
                                "invalid Unicode surrogate pair",
                            ));
                        }
                        c = surrogate_pair(c, low);
                        stop = next;
                    } else if (0xdc00..=0xdfff).contains(&c) {
                        return Err(Error::near(
                            self.sql,
                            at,
                            end,
                            "invalid Unicode surrogate pair",
                        ));
                    }
                    let ch = char::from_u32(c).filter(|&ch| ch != '\0').ok_or_else(|| {
                        Error::near(self.sql, at, end, "invalid Unicode escape value")
                    })?;
                    literal.extend_from_slice(ch.encode_utf8(&mut [0; 4]).as_bytes());
                    at = stop;
                    continue;
                }
                28 => return Err(Error::at(self.sql, at, "invalid Unicode escape", "22025")),
                29 => {
                    if matched[1] == b'\'' && self.options.disable_backslash_quote {
                        return Err(Error::at(
                            self.sql,
                            start,
                            "unsafe use of \\' in a string literal",
                            "22P06",
                        ));
                    }
                    literal.push(match matched[1] {
                        b'b' => 8,
                        b'f' => 12,
                        b'n' => 10,
                        b'r' => 13,
                        b't' => 9,
                        b'v' => 11,
                        c => c,
                    });
                }
                30 => literal.push(digits(&matched[1..], 8) as u8),
                31 => literal.push(digits(&matched[2..], 16) as u8),
                41 | 42 => {
                    if literal.is_empty() {
                        return Err(Error::near(
                            self.sql,
                            start,
                            end,
                            "zero-length delimited identifier",
                        ));
                    }
                    let ident = text(literal)?;
                    return Ok((
                        kind,
                        if kind == Kind::IDENT {
                            truncate_identifier(ident)
                        } else {
                            ident
                        },
                        end,
                    ));
                }
                43 => literal.push(b'"'),
                _ => unreachable!("unmapped quoted scanner action {action}"),
            }
            at = end;
        }
        Err(Error::near(self.sql, start, at, unterminated))
    }
}

pub(crate) fn surrogate_pair(high: u32, low: u32) -> u32 {
    0x10000 + ((high - 0xd800) << 10) + (low - 0xdc00)
}

pub(crate) fn digits(bytes: &[u8], radix: u32) -> u32 {
    bytes.iter().fold(0, |n, &b| {
        n * radix
            + char::from(b)
                .to_digit(radix)
                .expect("validated escape digits")
    })
}

fn text(bytes: Vec<u8>) -> Result<String, Error> {
    let invalid = std::str::from_utf8(&bytes).err().map(|e| e.valid_up_to());
    let zero = bytes.iter().position(|&b| b == 0);
    if let Some(i) = invalid.into_iter().chain(zero).min() {
        let len = match bytes[i] {
            b if b & 0xe0 == 0xc0 => 2,
            b if b & 0xf0 == 0xe0 => 3,
            b if b & 0xf8 == 0xf0 => 4,
            _ => 1,
        };
        let hex = bytes[i..]
            .iter()
            .take(len)
            .map(|b| format!("0x{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        return Err(Error {
            sqlstate: "22021",
            position: 0,
            message: format!("invalid byte sequence for encoding \"UTF8\": {hex}"),
        });
    }
    Ok(String::from_utf8(bytes).expect("validated literal"))
}
