use crate::{token, Error, Options, ParseMode, Value};
use pgsql_lexer::{Kind, Scanner};
use std::collections::VecDeque;
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lookup {
    Normal,
    Declare,
    Expression,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolved {
    pub datum: usize,
    /// For an unqualified record.field.subfield, PL/pgSQL uses record.field as the datum.
    pub names_used: usize,
}

pub trait Namespace {
    fn lookup(&mut self, names: &[String]) -> Option<Resolved>;
}

impl Namespace for () {
    fn lookup(&mut self, _: &[String]) -> Option<Resolved> {
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub id: u16,
    pub span: Range<usize>,
    pub value: Value,
    pub names: Vec<String>,
    pub quoted: bool,
    pub datum: Option<usize>,
}

impl Token {
    pub(crate) fn is_keyword(&self, id: u16, name: &str) -> bool {
        self.id == id
            || (self.id == token::T_DATUM
                && !self.quoted
                && self.names.len() == 1
                && self.names[0] == name)
    }
}

fn keyword(word: &str, list: &[(&'static str, u16)]) -> Option<(&'static str, u16)> {
    list.binary_search_by(|entry| entry.0.cmp(word))
        .ok()
        .map(|i| list[i])
}

pub struct BodyScanner<'a> {
    source: &'a str,
    core: Scanner<'a>,
    line_breaks: Vec<usize>,
    latest_line: std::cell::Cell<usize>,
    queue: VecDeque<Token>,
    previous_id: u16,
    last: Range<usize>,
    pub lookup: Lookup,
}

impl<'a> BodyScanner<'a> {
    pub fn new(source: &'a str, options: Options) -> Self {
        let source = source.split('\0').next().unwrap();
        Self {
            source,
            core: Scanner::with_options(source, options),
            latest_line: std::cell::Cell::new(1),
            line_breaks: source.match_indices('\n').map(|(i, _)| i).collect(),
            queue: VecDeque::new(),
            previous_id: 0,
            last: 0..0,
            lookup: Lookup::Normal,
        }
    }

    pub fn source(&self) -> &'a str {
        self.source
    }

    pub fn error(&self, message: &str) -> Error {
        Error::near(self.source, self.last.start, self.last.end, message)
    }

    pub fn line(&self, offset: usize) -> usize {
        let number = 1 + self.line_breaks.partition_point(|end| *end < offset);
        self.latest_line.set(number);
        number
    }

    pub(crate) fn latest_line(&self) -> usize {
        self.latest_line.get()
    }

    fn read_core(&mut self) -> Result<Token, Error> {
        let input = self
            .core
            .by_ref()
            .find(|t| t.as_ref().map_or(true, |t| !t.kind.is_comment()))
            .transpose()?;
        let mut result = Token {
            id: 0,
            span: self.source.len()..self.source.len(),
            value: Value::None,
            quoted: false,
            names: Vec::new(),
            datum: None,
        };
        let Some(input) = input else {
            return Ok(result);
        };
        result.span = input.start..input.end;
        let text = &self.source[result.span.clone()];
        result.id = input.kind.id();
        result.value = input.value;
        if input.keyword.is_some() || input.kind == Kind::IDENT || input.kind == Kind::PARAM {
            result.quoted = text.starts_with('"');
            let name = match &result.value {
                Value::Text(s) => s.clone(),
                Value::Keyword(_) => text.to_ascii_lowercase(),
                _ => text.to_owned(),
            };
            result.id = token::IDENT;
            if !result.quoted {
                if let Some((word, id)) = keyword(&name, token::RESERVED) {
                    result.id = id;
                    result.value = Value::Keyword(word);
                    return Ok(result);
                }
            }
            if input.kind == Kind::PARAM {
                result.id = token::PARAM;
            }
            result.names.push(name);
        } else if input.kind == Kind::OP {
            result.id = match text {
                "<<" => token::LESS_LESS,
                ">>" => token::GREATER_GREATER,
                "#" => u16::from(b'#'),
                _ => result.id,
            };
        }
        Ok(result)
    }

    pub fn push_back(&mut self, token: Token) {
        self.queue.push_front(token);
    }

    /// Mirrors plpgsql_peek/plpgsql_peek2. Raw lookahead precedes variable and unreserved-word lookup.
    pub fn peek_raw(&mut self, n: usize) -> Result<&Token, Error> {
        while self.queue.len() <= n {
            let token = self.read_core()?;
            self.queue.push_back(token);
        }
        Ok(&self.queue[n])
    }

    fn raw(&mut self) -> Result<Token, Error> {
        match self.queue.pop_front() {
            Some(token) => Ok(token),
            None => self.read_core(),
        }
    }

    pub fn next_token(&mut self, namespace: &mut impl Namespace) -> Result<Token, Error> {
        let mut t = self.raw()?;
        if matches!(t.id, token::IDENT | token::PARAM) {
            while t.names.len() < 3
                && self.peek_raw(0)?.id == u16::from(b'.')
                && self.peek_raw(1)?.id == token::IDENT
            {
                self.queue.pop_front();
                let mut part = self.queue.pop_front().unwrap();
                t.names.append(&mut part.names);
                t.span.end = part.span.end;
            }
            let compound = t.names.len() > 1;
            let eligible = if compound {
                self.lookup != Lookup::Declare
            } else {
                let stmt_start = matches!(
                    self.previous_id,
                    59 | token::K_BEGIN | token::K_THEN | token::K_ELSE | token::K_LOOP
                );
                let next = self.peek_raw(0)?.id;
                self.lookup == Lookup::Normal
                    && (!stmt_start || matches!(next, 46 | 61 | 91 | token::COLON_EQUALS))
            };
            let resolved = eligible.then(|| namespace.lookup(&t.names)).flatten();
            t.quoted &= !compound;
            if let Some(r) = resolved {
                t.id = token::T_DATUM;
                t.names.truncate(r.names_used);
                t.datum = Some(r.datum);
            } else if compound {
                t.id = token::T_CWORD;
            } else if let Some((word, id)) =
                keyword(&t.names[0], token::UNRESERVED).filter(|_| !t.quoted)
            {
                t.id = id;
                t.value = Value::Keyword(word);
            } else {
                t.id = token::T_WORD;
            }
        }
        self.last = t.span.clone();
        self.previous_id = t.id;
        Ok(t)
    }

    pub fn read_sql(
        &mut self,
        namespace: &mut impl Namespace,
        terminators: &[u16],
        expected: &str,
        mode: ParseMode,
        is_expression: bool,
    ) -> Result<SqlFragment<'a>, Error> {
        let saved = std::mem::replace(&mut self.lookup, Lookup::Expression);
        let result = self.sql_tokens(namespace, terminators, expected, mode, is_expression);
        self.lookup = saved;
        result
    }

    fn sql_tokens(
        &mut self,
        namespace: &mut impl Namespace,
        terminators: &[u16],
        expected: &str,
        mode: ParseMode,
        is_expression: bool,
    ) -> Result<SqlFragment<'a>, Error> {
        let mut span: Option<Range<usize>> = None;
        let mut depth = 0;
        loop {
            let token = self.next_token(namespace)?;
            let at = &token.span;
            let near = |message| Error::near(self.source, at.start, at.end, message);
            // Zero denotes an unused alternate terminator in read_sql_construct.
            let ends = terminators.contains(&token.id) || (token.id == 0 && terminators.len() < 3);
            if depth == 0 && ends {
                let span = span.ok_or_else(|| {
                    near(if is_expression {
                        "missing expression"
                    } else {
                        "missing SQL statement"
                    })
                })?;
                return Ok(SqlFragment {
                    query: &self.source[span.clone()],
                    mode,
                    span,
                    terminator: token,
                });
            }
            match token.id {
                40 | 91 => depth += 1,
                41 | 93 if depth == 0 => return Err(near("mismatched parentheses")),
                41 | 93 => depth -= 1,
                0 | 59 if depth != 0 => return Err(near("mismatched parentheses")),
                0 | 59 => {
                    let construct = if is_expression {
                        "expression"
                    } else {
                        "statement"
                    };
                    return Err(Error::at(
                        self.source,
                        at.start,
                        format!("missing \"{expected}\" at end of SQL {construct}"),
                        "42601",
                    ));
                }
                _ => {}
            }
            span.get_or_insert_with(|| at.clone()).end = at.end;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlFragment<'a> {
    pub query: &'a str,
    pub mode: ParseMode,
    pub span: Range<usize>,
    pub terminator: Token,
}
