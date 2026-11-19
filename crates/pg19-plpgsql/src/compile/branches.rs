use super::*;

impl<S: EmbeddedSql> Compiler<'_, S> {
    pub(super) fn branch(
        &mut self,
        lhs: &str,
        prod: &str,
        rhs: &mut Reduction<Reduced>,
        s: &mut BodyScanner,
    ) -> Result<Reduced, CompileError> {
        Ok(match lhs {
            "opt_expr_until_when" => {
                let t = s.next_token(self)?;
                if t.id == token::K_WHEN {
                    s.push_back(t);
                    Reduced::None
                } else {
                    s.push_back(t);
                    let f =
                        s.read_sql(self, &[token::K_WHEN], "WHEN", ParseMode::PlPgSqlExpr, true)?;
                    self.sql
                        .validate(f.query, ParseMode::PlPgSqlExpr)
                        .map_err(|e| e.in_body(s.source(), f.span.start))?;
                    s.push_back(f.terminator);
                    Reduced::Expression(Expression {
                        query: f.query.into(),
                        mode: ParseMode::PlPgSqlExpr,
                    })
                }
            }
            "case_when" => Reduced::Elsifs(vec![crate::Elsif {
                lineno: s.line(rhs.location(1)),
                condition: rhs.take(2).expr(),
                body: rhs.take(3).list(),
            }]),
            "case_when_list" if prod == "case_when" => rhs.take(1),
            "case_when_list" => {
                let Reduced::Elsifs(mut list) = rhs.take(1) else {
                    unreachable!()
                };
                let Reduced::Elsifs(tail) = rhs.take(2) else {
                    unreachable!()
                };
                list.extend(tail);
                Reduced::Elsifs(list)
            }
            "opt_case_else" if prod == "%empty" => Reduced::None,
            "opt_case_else" => Reduced::List(rhs.take(2).list()),
            "stmt_case" => {
                let lineno = s.line(rhs.location(1));
                let Reduced::Elsifs(mut branches) = rhs.take(3) else {
                    unreachable!()
                };
                let mut datum = 0;
                let operand = if let Reduced::Expression(operand) = rhs.take(2) {
                    datum = self.env.datums.len();
                    let name = format!("__Case__Variable_{datum}__");
                    let mut v = Variable::implicit(&name, DataType::Scalar("int4".into()));
                    v.lineno = lineno;
                    self.env.declare(v);
                    for b in &mut branches {
                        b.condition.query = format!("\"{name}\" IN ({})", b.condition.query);
                    }
                    Some(operand)
                } else {
                    None
                };
                let otherwise = match rhs.take(4) {
                    Reduced::None => None,
                    other => Some(other.list()),
                };
                Reduced::Statement(Statement::Case {
                    lineno,
                    datum,
                    operand,
                    branches,
                    otherwise,
                })
            }
            "foreach_slice" => {
                let slice = if prod == "%empty" {
                    0
                } else {
                    let crate::Value::Integer(value) = rhs.take(2).token().value else {
                        unreachable!()
                    };
                    value
                };
                Reduced::Number(slice)
            }
            "stmt_foreach_a" => {
                let lineno = s.line(rhs.location(2));
                let label = rhs.take(1).name();
                let Reduced::LoopVariable(t, row) = rhs.take(3) else {
                    unreachable!()
                };
                let datum = if let Some(row) = row {
                    row.id
                } else if let Some(id) = t.datum {
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
                    id
                } else {
                    return Err(Error::at(
                        s.source(),
                        t.span.start,
                        "loop variable of FOREACH must be a known variable or list of variables",
                        "42601",
                    )
                    .into());
                };
                let Reduced::Number(slice) = rhs.take(4) else {
                    unreachable!()
                };
                let Reduced::LoopBody { body, end, at } = rhs.take(8) else {
                    unreachable!()
                };
                if let Some(end) = end {
                    check_label(label.as_deref(), &end, at, s.source())?;
                }
                self.env.pop();
                Reduced::Statement(Statement::Foreach {
                    lineno,
                    label,
                    datum,
                    slice,
                    expression: rhs.take(7).expr(),
                    body,
                })
            }
            _ => unreachable!(),
        })
    }
}
