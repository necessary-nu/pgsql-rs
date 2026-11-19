use super::*;
use crate::{Datum, Target};

impl<S: EmbeddedSql> Compiler<'_, S> {
    pub(super) fn read_into_target(&mut self, s: &mut BodyScanner) -> Result<Target, CompileError> {
        let mut t = s.next_token(self)?;
        let strict = t.is_keyword(token::K_STRICT, "strict");
        if strict {
            t = s.next_token(self)?;
        }
        let at = t.span.start;
        let mut fields = Vec::new();
        let id = loop {
            let name = t.names.join(".");
            let id = t.datum.ok_or_else(|| {
                if matches!(t.id, token::T_WORD | token::T_CWORD) {
                    Error::at(
                        s.source(),
                        t.span.start,
                        format!("\"{name}\" is not a known variable"),
                        "42601",
                    )
                } else {
                    s.error("syntax error")
                }
            })?;
            let var = self.env.variable(id);
            if var.constant {
                return Err(Error::at(
                    s.source(),
                    t.span.start,
                    format!("variable \"{}\" is declared CONSTANT", var.name),
                    "22005",
                )
                .into());
            }
            let record =
                matches!(&self.env.datums[id],Datum::Variable(v) if v.datatype==DataType::Record);
            if record && !fields.is_empty() {
                return Err(Error::at(
                    s.source(),
                    t.span.start,
                    format!("\"{name}\" is not a scalar variable"),
                    "42601",
                )
                .into());
            }
            let next = s.next_token(self)?;
            if next.id != 44 {
                s.push_back(next);
                if record {
                    break id;
                }
                fields.push((name, id));
                let index = self.env.datums.len();
                self.env.datums.push(Datum::Row {
                    lineno: s.line(at) as i32,
                    fields,
                });
                break index;
            }
            if record {
                return Err(Error::at(
                    s.source(),
                    next.span.start,
                    "record variable cannot be part of multiple-item INTO list",
                    "42601",
                )
                .into());
            }
            fields.push((name, id));
            if fields.len() >= 1024 {
                return Err(Error::at(
                    s.source(),
                    next.span.start,
                    "too many INTO variables specified",
                    "54000",
                )
                .into());
            }
            t = s.next_token(self)?;
        };
        Ok(Target {
            id,
            variable: self.env.datums[id].clone(),
            strict,
        })
    }

    pub(super) fn return_set(
        &mut self,
        location: usize,
        s: &mut BodyScanner,
        next: bool,
    ) -> Result<Reduced, CompileError> {
        let Returns::Set { out } = self.returns else {
            let msg = if next {
                "cannot use RETURN NEXT in a non-SETOF function"
            } else {
                "cannot use RETURN QUERY in a non-SETOF function"
            };
            return Err(Error::at(s.source(), location, msg, "42804").into());
        };
        let lineno = s.line(location);
        let t = s.next_token(self)?;
        if !next {
            let dynamic = t.is_keyword(token::K_EXECUTE, "execute");
            let mut params = Vec::new();
            let query = if dynamic {
                let (e, mut end) = self.fragment(
                    s,
                    &[59, token::K_USING],
                    "; or USING",
                    ParseMode::PlPgSqlExpr,
                )?;
                if end == token::K_USING {
                    loop {
                        let (p, term) =
                            self.fragment(s, &[44, 59], ", or ;", ParseMode::PlPgSqlExpr)?;
                        params.push(p);
                        end = term;
                        if end != 44 {
                            break;
                        }
                    }
                }
                e
            } else {
                s.push_back(t);
                self.expression(s, ParseMode::Sql, false)?
            };
            return Ok(Reduced::Statement(Statement::ReturnQuery {
                lineno,
                query,
                dynamic,
                params,
            }));
        }
        let mut datum = out;
        let mut expression = None;
        if out.is_some() {
            if t.id != 59 {
                return Err(Error::at(
                    s.source(),
                    t.span.start,
                    "RETURN NEXT cannot have a parameter in function with OUT parameters",
                    "42804",
                )
                .into());
            }
        } else if t
            .datum
            .is_some_and(|id| !matches!(self.env.datums[id], Datum::RecordField { .. }))
            && s.peek_raw(0)?.id == 59
        {
            datum = t.datum;
            s.next_token(self)?;
        } else {
            s.push_back(t);
            expression = Some(self.expression(s, ParseMode::PlPgSqlExpr, false)?);
        }
        Ok(Reduced::Statement(Statement::ReturnNext {
            lineno,
            datum,
            expression,
        }))
    }
}
