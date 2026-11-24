use super::*;
use crate::{ExceptionHandler, Value};

const CONDITIONS: &[&str] = include!("../../data/conditions.in");
const LEVELS: &[(u16, &str, u8)] = &[
    (token::K_EXCEPTION, "exception", 21),
    (token::K_WARNING, "warning", 19),
    (token::K_NOTICE, "notice", 18),
    (token::K_INFO, "info", 17),
    (token::K_LOG, "log", 15),
    (token::K_DEBUG, "debug", 14),
];
const OPTIONS: &[(u16, &str)] = &[
    (token::K_ERRCODE, "errcode"),
    (token::K_MESSAGE, "message"),
    (token::K_DETAIL, "detail"),
    (token::K_HINT, "hint"),
    (token::K_COLUMN, "column"),
    (token::K_CONSTRAINT, "constraint"),
    (token::K_DATATYPE, "datatype"),
    (token::K_TABLE, "table"),
    (token::K_SCHEMA, "schema"),
];

fn lookup(name: &str) -> Result<usize, CompileError> {
    let count =
        CONDITIONS.partition_point(|v| *v <= name) - CONDITIONS.partition_point(|v| *v < name);
    if count == 0 {
        return Err(Error::without_position(
            format!("unrecognized exception condition \"{name}\""),
            "42704",
        )
        .into());
    }
    Ok(count)
}

fn check_parameters(message: &str, count: usize) -> Result<(), CompileError> {
    let mut chars = message.bytes().peekable();
    let mut expected = 0;
    while let Some(c) = chars.next() {
        if c == b'%' && chars.next_if_eq(&b'%').is_none() {
            expected += 1;
        }
    }
    let why = match count.cmp(&expected) {
        std::cmp::Ordering::Equal => return Ok(()),
        std::cmp::Ordering::Less => "few",
        std::cmp::Ordering::Greater => "many",
    };
    Err(
        Error::without_position(format!("too {why} parameters specified for RAISE"), "42601")
            .into(),
    )
}

impl<S: EmbeddedSql> Compiler<'_, S> {
    fn sqlstate(&mut self, s: &mut BodyScanner) -> Result<String, CompileError> {
        let t = s.next_token(self)?;
        if t.id != token::SCONST {
            return Err(s.error("syntax error").into());
        }
        let Value::Text(name) = t.value else {
            unreachable!()
        };
        if name.len() != 5
            || !name
                .bytes()
                .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase())
        {
            return Err(s.error("invalid SQLSTATE code").into());
        }
        Ok(name)
    }

    fn raise_options(
        &mut self,
        s: &mut BodyScanner,
    ) -> Result<Vec<(u8, Expression)>, CompileError> {
        let mut options = Vec::new();
        loop {
            let t = s.next_token(self)?;
            if t.id == 0 {
                return Err(s.error("unexpected end of function definition").into());
            }
            let Some(option) = OPTIONS
                .iter()
                .position(|(id, name)| t.is_keyword(*id, name))
            else {
                return Err(s.error("unrecognized RAISE statement option").into());
            };
            if !matches!(s.next_token(self)?.id, 61 | token::COLON_EQUALS) {
                return Err(s.error("syntax error, expected \"=\"").into());
            }
            let (e, end) = self.fragment(s, &[44, 59], ", or ;", ParseMode::PlPgSqlExpr)?;
            options.push((option as u8, e));
            if end == 59 {
                return Ok(options);
            }
        }
    }

    pub(super) fn raise(
        &mut self,
        at: usize,
        s: &mut BodyScanner,
    ) -> Result<Reduced, CompileError> {
        let lineno = s.line(at);
        let mut t = s.next_token(self)?;
        let mut level = 21;
        let mut condition = None;
        let mut message = None;
        let mut params = Vec::new();
        let mut end = t.id;
        if end != 59 {
            if let Some(&(_, _, severity)) =
                LEVELS.iter().find(|(id, name, _)| t.is_keyword(*id, name))
            {
                level = severity;
                t = s.next_token(self)?;
            }
            if t.id == 0 {
                return Err(s.error("unexpected end of function definition").into());
            }
            end = match t.id {
                token::SCONST => {
                    let Value::Text(format) = t.value else {
                        unreachable!()
                    };
                    message = Some(format);
                    let mut end = s.next_token(self)?.id;
                    while end == 44 {
                        let (param, after) = self.fragment(
                            s,
                            &[44, 59, token::K_USING],
                            ", or ; or USING",
                            ParseMode::PlPgSqlExpr,
                        )?;
                        params.push(param);
                        end = after;
                    }
                    end
                }
                token::K_USING => token::K_USING,
                _ => {
                    condition = Some(if t.is_keyword(token::K_SQLSTATE, "sqlstate") {
                        self.sqlstate(s)?
                    } else if t.id == token::T_WORD || token::UNRESERVED.iter().any(|k| k.1 == t.id)
                    {
                        let name = t.names.into_iter().next().unwrap();
                        lookup(&name)?;
                        name
                    } else {
                        return Err(s.error("syntax error").into());
                    });
                    s.next_token(self)?.id
                }
            };
        }
        let options = match end {
            token::K_USING => self.raise_options(s)?,
            59 => Vec::new(),
            _ => return Err(s.error("syntax error").into()),
        };
        if let Some(msg) = &message {
            check_parameters(msg, params.len())?;
        }
        Ok(Reduced::Statement(Statement::Raise {
            lineno,
            level,
            condition,
            message,
            params,
            options,
        }))
    }

    pub(super) fn exceptions(
        &mut self,
        lhs: &str,
        production: &str,
        rhs: &mut Reduction<Reduced>,
        s: &mut BodyScanner,
    ) -> Result<Reduced, CompileError> {
        Ok(match lhs {
            "@2" => {
                let lineno = s.line(rhs.location(0));
                for name in ["sqlstate", "sqlerrm"] {
                    let mut v = Variable::implicit(name, DataType::Scalar("text".into()));
                    v.lineno = lineno;
                    v.constant = true;
                    self.env.declare(v);
                }
                Reduced::None
            }
            "exception_sect" => rhs.take(3),
            "proc_condition" => {
                let mut name = rhs.take(1).name().unwrap();
                let n = match name.as_str() {
                    "sqlstate" => {
                        name = self.sqlstate(s)?;
                        1
                    }
                    "others" => 1,
                    _ => lookup(&name)?,
                };
                Reduced::Conditions(vec![name; n])
            }
            "proc_conditions" if production.starts_with("proc_conditions ") => {
                let Reduced::Conditions(mut list) = rhs.take(1) else {
                    unreachable!()
                };
                let Reduced::Conditions(tail) = rhs.take(3) else {
                    unreachable!()
                };
                list.extend(tail);
                Reduced::Conditions(list)
            }
            "proc_exception" => {
                s.line(rhs.location(1));
                let Reduced::Conditions(conditions) = rhs.take(2) else {
                    unreachable!()
                };
                Reduced::Handlers(vec![ExceptionHandler {
                    conditions,
                    action: rhs.take(4).list(),
                }])
            }
            "proc_exceptions" if production.starts_with("proc_exceptions ") => {
                let Reduced::Handlers(mut list) = rhs.take(1) else {
                    unreachable!()
                };
                let Reduced::Handlers(tail) = rhs.take(2) else {
                    unreachable!()
                };
                list.extend(tail);
                Reduced::Handlers(list)
            }
            _ => rhs.take(1),
        })
    }
}
