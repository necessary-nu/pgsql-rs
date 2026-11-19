use super::*;
use crate::{Datum, ForSource, Target};

impl<S: EmbeddedSql> Compiler<'_, S> {
    pub(super) fn for_variable(
        &mut self,
        t: Token,
        s: &mut BodyScanner,
    ) -> Result<Reduced, CompileError> {
        s.line(t.span.start);
        if t.id == token::T_CWORD {
            return Err(Error::at(
                s.source(),
                t.span.start,
                format!("\"{}\" is not a known variable", t.names.join(".")),
                "42601",
            )
            .into());
        }
        let record = t.datum.is_some_and(
            |id| matches!(&self.env.datums[id],Datum::Variable(v) if v.datatype==DataType::Record),
        );
        let target = if !record && s.peek_raw(0)?.id == 44 {
            s.push_back(t.clone());
            Some(Box::new(self.read_into_target(s)?))
        } else {
            None
        };
        Ok(Reduced::LoopVariable(t, target))
    }

    fn make_target(&self, id: usize) -> Box<Target> {
        Box::new(Target {
            id,
            variable: self.env.datums[id].clone(),
            strict: false,
        })
    }

    fn query_target(
        &mut self,
        t: Token,
        prior: Option<Box<Target>>,
        s: &BodyScanner,
    ) -> Result<Box<Target>, CompileError> {
        if let Some(target) = prior {
            return Ok(target);
        }
        let Some(id) = t.datum else {
            return Err(Error::at(s.source(),t.span.start,
            "loop variable of loop over rows must be a record variable or list of scalar variables","42601").into());
        };
        let v = self.env.variable(id);
        if v.constant {
            return Err(Error::at(
                s.source(),
                t.span.start,
                format!("variable \"{}\" is declared CONSTANT", v.name),
                "22005",
            )
            .into());
        }
        if matches!(&self.env.datums[id],Datum::Variable(v) if v.datatype==DataType::Record) {
            return Ok(self.make_target(id));
        }
        let row = self.env.datums.len();
        self.env.datums.push(Datum::Row {
            lineno: s.line(t.span.start) as i32,
            fields: vec![(t.names.join("."), id)],
        });
        Ok(self.make_target(row))
    }

    pub(super) fn for_control(
        &mut self,
        rhs: &mut Reduction<Reduced>,
        s: &mut BodyScanner,
    ) -> Result<Reduced, CompileError> {
        let Reduced::LoopVariable(var, prior) = rhs.take(1) else {
            unreachable!()
        };
        let first = s.next_token(self)?;
        let at = first.span.start;
        let expr = ParseMode::PlPgSqlExpr;
        if first.id == token::K_EXECUTE {
            let (query, end) =
                self.fragment(s, &[token::K_LOOP, token::K_USING], "LOOP or USING", expr)?;
            let target = self.query_target(var, prior, s)?;
            let mut params = Vec::new();
            if end == token::K_USING {
                loop {
                    let (p, e) = self.fragment(s, &[44, token::K_LOOP], ", or LOOP", expr)?;
                    params.push(p);
                    if e != 44 {
                        break;
                    }
                }
            }
            return Ok(Reduced::For(
                target,
                Box::new(ForSource::Query {
                    query,
                    dynamic: true,
                    params,
                }),
            ));
        }
        if let Some(cursor) = first.datum.filter(|&id| matches!(&self.env.variable(id).datatype, DataType::Scalar(name) if name == "refcursor")) {
            if prior.is_some() { return Err(Error::at(s.source(),var.span.start,"cursor FOR loop must have only one target variable","42601").into()); }
            if !matches!(self.env.datums[cursor], Datum::Cursor{..}) {
                return Err(Error::at(s.source(),at,"cursor FOR loop must use a bound cursor variable","42601").into());
            }
            let argquery=self.cursor_args(cursor,token::K_LOOP,s)?;
            let mut row=Variable::implicit(var.names.join("."),DataType::Record);
            row.lineno=s.line(var.span.start);
            let id=self.env.datums.len();
            self.env.declare(row);
            return Ok(Reduced::For(self.make_target(id),Box::new(ForSource::Cursor{cursor,argquery})));
        }
        let reverse = first.is_keyword(token::K_REVERSE, "reverse");
        if !reverse {
            s.push_back(first);
        }
        let f = s.read_sql(
            self,
            &[token::DOT_DOT, token::K_LOOP],
            "LOOP",
            ParseMode::Sql,
            true,
        )?;
        let integer = f.terminator.id == token::DOT_DOT;
        if reverse && !integer {
            return Err(Error::at(
                s.source(),
                at,
                "cannot specify REVERSE in query FOR loop",
                "42601",
            )
            .into());
        }
        let mode = if integer { expr } else { ParseMode::Sql };
        self.sql
            .validate(f.query, mode)
            .map_err(|e| e.in_body(s.source(), f.span.start))?;
        let first = Expression {
            query: f.query.into(),
            mode,
        };
        let (target, source) = if integer {
            let (upper, end) = self.fragment(s, &[token::K_LOOP, token::K_BY], "LOOP", expr)?;
            let step = if end == token::K_BY {
                Some(self.fragment(s, &[token::K_LOOP], "LOOP", expr)?.0)
            } else {
                None
            };
            if prior.is_some() {
                return Err(Error::at(
                    s.source(),
                    var.span.start,
                    "integer FOR loop must have only one target variable",
                    "42601",
                )
                .into());
            }
            let mut variable =
                Variable::implicit(var.names.join("."), DataType::Scalar("int4".into()));
            variable.lineno = s.line(var.span.start);
            let id = self.env.datums.len();
            self.env.declare(variable);
            (
                self.make_target(id),
                ForSource::Integer {
                    lower: first,
                    upper,
                    step,
                    reverse,
                },
            )
        } else {
            (
                self.query_target(var, prior, s)?,
                ForSource::Query {
                    query: first,
                    dynamic: false,
                    params: Vec::new(),
                },
            )
        };
        Ok(Reduced::For(target, Box::new(source)))
    }

    pub(super) fn finish_for(
        &mut self,
        rhs: &mut Reduction<Reduced>,
        s: &BodyScanner,
    ) -> Result<Reduced, CompileError> {
        let label = rhs.take(1).name();
        let Reduced::For(target, source) = rhs.take(3) else {
            unreachable!()
        };
        let Reduced::LoopBody { body, end, at } = rhs.take(4) else {
            unreachable!()
        };
        let lineno = s.line(rhs.location(2));
        if let Some(end) = end {
            check_label(label.as_deref(), &end, at, s.source())?;
        }
        self.env.pop();
        Ok(Reduced::Statement(Statement::For {
            lineno,
            label,
            target,
            source,
            body,
        }))
    }

    pub(super) fn dynamic_execute(
        &mut self,
        at: usize,
        s: &mut BodyScanner,
    ) -> Result<Reduced, CompileError> {
        let expr = ParseMode::PlPgSqlExpr;
        let (query, mut end) = self.fragment(
            s,
            &[token::K_INTO, token::K_USING, 59],
            "INTO or USING or ;",
            expr,
        )?;
        let lineno = s.line(at);
        let mut target = None;
        let mut params = Vec::new();
        loop {
            match end {
                token::K_INTO if target.is_none() => {
                    target = Some(Box::new(self.read_into_target(s)?));
                    end = s.next_token(self)?.id;
                }
                token::K_USING if params.is_empty() => loop {
                    let (p, t) =
                        self.fragment(s, &[44, 59, token::K_INTO], ", or ; or INTO", expr)?;
                    params.push(p);
                    end = t;
                    if t != 44 {
                        break;
                    }
                },
                59 => break,
                _ => return Err(s.error("syntax error").into()),
            }
        }
        Ok(Reduced::Statement(Statement::Execute {
            lineno,
            query,
            target,
            params,
        }))
    }
}
