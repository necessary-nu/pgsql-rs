use super::*;
use crate::{CursorQuery, Datum};
use pg19_lexer::{keyword, KeywordKind};

const SCROLL: i32 = 2;
const NO_SCROLL: i32 = 4;
const FAST_PLAN: i32 = 256;

fn quoted(name: &str) -> String {
    let plain = name.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        && name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
        && !keyword(name).is_some_and(|(_, _, kind)| kind != KeywordKind::Unreserved);
    if plain {
        name.to_owned()
    } else {
        format!("\"{}\"", name.replace('"', "\"\""))
    }
}

impl<S: EmbeddedSql> Compiler<'_, S> {
    pub(super) fn cursor_args(
        &mut self,
        cursor: usize,
        until: u16,
        s: &mut BodyScanner,
    ) -> Result<Option<Expression>, CompileError> {
        let t = s.next_token(self)?;
        let Datum::Cursor { variable, args, .. } = &self.env.datums[cursor] else {
            unreachable!()
        };
        let name = variable.name.clone();
        let Some(row) = args else {
            if t.id == 40 {
                return Err(Error::at(
                    s.source(),
                    t.span.start,
                    format!("cursor \"{name}\" has no arguments"),
                    "42601",
                )
                .into());
            }
            if t.id != until {
                return Err(s.error("syntax error").into());
            }
            return Ok(None);
        };
        if t.id != 40 {
            return Err(Error::at(
                s.source(),
                t.span.start,
                format!("cursor \"{name}\" has arguments"),
                "42601",
            )
            .into());
        }
        let Datum::Row { fields, .. } = &self.env.datums[*row] else {
            unreachable!()
        };
        let names: Vec<_> = fields.iter().map(|(name, _)| name.clone()).collect();
        let mut values: Vec<Option<String>> = vec![None; names.len()];
        let mut any_named = false;
        for i in 0..names.len() {
            let at = s.peek_raw(0)?.span.start;
            let index = if s.peek_raw(0)?.id == token::IDENT
                && matches!(
                    s.peek_raw(1)?.id,
                    token::COLON_EQUALS | token::EQUALS_GREATER
                ) {
                let saved = std::mem::replace(&mut s.lookup, Lookup::Declare);
                let arg = s.next_token(self);
                s.lookup = saved;
                let arg = arg?.names.into_iter().next().unwrap();
                let Some(index) = names.iter().position(|n| *n == arg) else {
                    return Err(Error::at(
                        s.source(),
                        at,
                        format!("cursor \"{name}\" has no argument named \"{arg}\""),
                        "42601",
                    )
                    .into());
                };
                s.next_token(self)?;
                any_named = true;
                index
            } else {
                i
            };
            if values[index].is_some() {
                return Err(Error::at(
                    s.source(),
                    at,
                    format!(
                        "value for parameter \"{}\" of cursor \"{name}\" specified more than once",
                        names[index]
                    ),
                    "42601",
                )
                .into());
            }
            let f = s.read_sql(self, &[44, 41], ",\" or \")", ParseMode::PlPgSqlExpr, true)?;
            self.sql
                .validate(f.query, ParseMode::PlPgSqlExpr)
                .map_err(|e| e.in_body(s.source(), f.span.start))?;
            values[index] = Some(f.query.into());
            let last = i + 1 == names.len();
            let problem = match (f.terminator.id, last) {
                (41, false) => Some("not enough"),
                (44, true) => Some("too many"),
                _ => None,
            };
            if let Some(problem) = problem {
                return Err(Error::at(
                    s.source(),
                    f.terminator.span.start,
                    format!("{problem} arguments for cursor \"{name}\""),
                    "42601",
                )
                .into());
            }
        }
        if s.next_token(self)?.id != until {
            return Err(s.error("syntax error").into());
        }
        let args: Vec<_> = values
            .into_iter()
            .zip(names)
            .map(|(value, name)| {
                let value = value.unwrap();
                if any_named {
                    format!("{value} AS {}", quoted(&name))
                } else {
                    value
                }
            })
            .collect();
        Ok(Some(Expression {
            query: args.join(", "),
            mode: ParseMode::PlPgSqlExpr,
        }))
    }

    fn open_cursor(
        &mut self,
        cursor: usize,
        lineno: usize,
        s: &mut BodyScanner,
    ) -> Result<Statement, CompileError> {
        let mut options = FAST_PLAN;
        let query = if matches!(self.env.datums[cursor], Datum::Cursor { .. }) {
            CursorQuery::Bound(self.cursor_args(cursor, 59, s)?)
        } else {
            let mut t = s.next_token(self)?;
            if t.is_keyword(token::K_NO, "no") {
                t = s.next_token(self)?;
                if t.is_keyword(token::K_SCROLL, "scroll") {
                    options |= NO_SCROLL;
                    t = s.next_token(self)?;
                }
            } else if t.is_keyword(token::K_SCROLL, "scroll") {
                options |= SCROLL;
                t = s.next_token(self)?;
            }
            if t.id != token::K_FOR {
                return Err(s.error("syntax error, expected \"FOR\"").into());
            }
            let t = s.next_token(self)?;
            if t.is_keyword(token::K_EXECUTE, "execute") {
                let (query, end) = self.fragment(
                    s,
                    &[token::K_USING, 59],
                    "USING or ;",
                    ParseMode::PlPgSqlExpr,
                )?;
                let mut params = Vec::new();
                if end == token::K_USING {
                    loop {
                        let (e, end) =
                            self.fragment(s, &[44, 59], ", or ;", ParseMode::PlPgSqlExpr)?;
                        params.push(e);
                        if end == 59 {
                            break;
                        }
                    }
                }
                CursorQuery::Dynamic(query, params)
            } else {
                s.push_back(t);
                CursorQuery::Static(self.fragment(s, &[59], ";", ParseMode::Sql)?.0)
            }
        };
        Ok(Statement::Open {
            lineno,
            cursor,
            options,
            query,
        })
    }

    pub(super) fn cursor(
        &mut self,
        lhs: &str,
        prod: &str,
        rhs: &mut Reduction<Reduced>,
        s: &mut BodyScanner,
    ) -> Result<Reduced, CompileError> {
        Ok(match lhs {
            "opt_scrollable" => Reduced::Number(match prod {
                "K_SCROLL" => SCROLL,
                "K_NO K_SCROLL" => NO_SCROLL,
                _ => 0,
            }),
            "$@1" => {
                let Reduced::Token(name) = rhs.value(-2) else {
                    unreachable!()
                };
                self.env.push(name.names.first().cloned());
                Reduced::None
            }
            "decl_cursor_query" => {
                Reduced::Expression(self.fragment(s, &[59], ";", ParseMode::Sql)?.0)
            }
            "decl_cursor_arg" => {
                let name = rhs.take(1).name().unwrap();
                let Reduced::Type(datatype) = rhs.take(2) else {
                    unreachable!()
                };
                if let DataType::Pseudo(pseudo) = &datatype {
                    return Err(Error::without_position(
                        format!("variable \"{name}\" has pseudo-type {pseudo}"),
                        "0A000",
                    )
                    .into());
                }
                let mut var = Variable::implicit(name, datatype);
                var.lineno = s.line(rhs.location(1));
                let id = self.env.datums.len();
                self.env.declare(var);
                Reduced::Datum(id)
            }
            "decl_cursor_arglist" => {
                let ids = if prod == "decl_cursor_arg" {
                    vec![rhs.take(1).datum()]
                } else {
                    let Reduced::Arguments(mut ids) = rhs.take(1) else {
                        unreachable!()
                    };
                    ids.push(rhs.take(3).datum());
                    ids
                };
                Reduced::Arguments(ids)
            }
            "decl_cursor_args" if prod == "%empty" => Reduced::None,
            "decl_cursor_args" => {
                let Reduced::Arguments(ids) = rhs.take(2) else {
                    unreachable!()
                };
                let fields = ids
                    .into_iter()
                    .map(|id| (self.env.variable(id).name.clone(), id))
                    .collect();
                let id = self.env.datums.len();
                self.env.datums.push(Datum::Row {
                    lineno: s.line(rhs.location(1)) as i32,
                    fields,
                });
                Reduced::Datum(id)
            }
            "decl_statement" => {
                self.env.pop();
                let name = rhs.take(1).name().unwrap();
                let mut variable =
                    Variable::implicit(name.clone(), DataType::Scalar("refcursor".into()));
                variable.lineno = s.line(rhs.location(1));
                let Reduced::Number(scroll) = rhs.take(2) else {
                    unreachable!()
                };
                let args = match rhs.take(5) {
                    Reduced::None => None,
                    r => Some(r.datum()),
                };
                self.env.alias(name, self.env.datums.len());
                self.env.datums.push(Datum::Cursor {
                    variable,
                    query: rhs.take(7).expr(),
                    args,
                    options: FAST_PLAN | scroll,
                });
                Reduced::None
            }
            "cursor_variable" => {
                let t = rhs.take(1).token();
                let name = t.names.join(".");
                let id = t.datum.ok_or_else(|| {
                    Error::at(
                        s.source(),
                        t.span.start,
                        format!("\"{name}\" is not a known variable"),
                        "42601",
                    )
                })?;
                let var = self.env.variable(id);
                if !matches!(&self.env.datums[id],Datum::Variable(v) if matches!(v.datatype,DataType::Scalar(_)))
                    && !matches!(self.env.datums[id], Datum::Cursor { .. })
                    || s.peek_raw(0)?.id == 91
                {
                    return Err(Error::at(
                        s.source(),
                        t.span.start,
                        "cursor variable must be a simple variable",
                        "42804",
                    )
                    .into());
                }
                if !matches!(&var.datatype,DataType::Scalar(ty) if ty=="refcursor") {
                    return Err(Error::at(
                        s.source(),
                        t.span.start,
                        format!(
                            "variable \"{}\" must be of type cursor or refcursor",
                            var.name
                        ),
                        "42804",
                    )
                    .into());
                }
                Reduced::Datum(id)
            }
            "stmt_open" => Reduced::Statement(self.open_cursor(
                rhs.take(2).datum(),
                s.line(rhs.location(1)),
                s,
            )?),
            "stmt_close" => Reduced::Statement(Statement::Close {
                lineno: s.line(rhs.location(1)),
                cursor: rhs.take(2).datum(),
            }),
            _ => unreachable!(),
        })
    }
}
