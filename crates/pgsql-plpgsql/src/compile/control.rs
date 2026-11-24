use super::*;

impl<S: EmbeddedSql> Compiler<'_, S> {
    pub(super) fn fragment(
        &mut self,
        s: &mut BodyScanner,
        ends: &[u16],
        expected: &str,
        mode: ParseMode,
    ) -> Result<(Expression, u16), CompileError> {
        let f = s.read_sql(self, ends, expected, mode, mode == ParseMode::PlPgSqlExpr)?;
        self.sql
            .validate(f.query, mode)
            .map_err(|e| e.in_body(s.source(), f.span.start))?;
        Ok((
            Expression {
                query: f.query.to_owned(),
                mode,
            },
            f.terminator.id,
        ))
    }

    pub(super) fn control(
        &mut self,
        lhs: &str,
        prod: &str,
        rhs: &mut Reduction<Reduced>,
        s: &mut BodyScanner,
        sig: &'static str,
    ) -> Result<Reduced, CompileError> {
        let expr = ParseMode::PlPgSqlExpr;
        let result = match lhs {
            "expr_until_then" | "expr_until_loop" | "expr_until_semi" => {
                let (id, text) = match lhs {
                    "expr_until_then" => (token::K_THEN, "THEN"),
                    "expr_until_loop" => (token::K_LOOP, "LOOP"),
                    _ => (59, ";"),
                };
                Reduced::Expression(self.fragment(s, &[id], text, expr)?.0)
            }
            "stmt_if" => {
                let Reduced::Elsifs(elsif) = rhs.take(4) else {
                    unreachable!()
                };
                Reduced::Statement(Statement::If {
                    lineno: s.line(rhs.location(1)),
                    condition: rhs.take(2).expr(),
                    then_body: rhs.take(3).list(),
                    elsif,
                    else_body: rhs.take(5).list(),
                })
            }
            "stmt_elsifs" if prod == "%empty" => Reduced::Elsifs(Vec::new()),
            "stmt_elsifs" => {
                let Reduced::Elsifs(mut items) = rhs.take(1) else {
                    unreachable!()
                };
                items.push(crate::Elsif {
                    lineno: s.line(rhs.location(2)),
                    condition: rhs.take(3).expr(),
                    body: rhs.take(4).list(),
                });
                Reduced::Elsifs(items)
            }
            "stmt_else" | "opt_exitcond" if prod.starts_with("K_") => rhs.take(2),
            "stmt_else" | "opt_exitcond" => Reduced::None,
            "opt_loop_label" => {
                let value = if prod == "%empty" {
                    Reduced::None
                } else {
                    rhs.take(2)
                };
                let label = match &value {
                    Reduced::Token(t) => t.names.first().cloned(),
                    _ => None,
                };
                self.env.push_loop(label);
                value
            }
            "loop_body" => Reduced::LoopBody {
                body: rhs.take(1).list(),
                end: rhs.take(4).name(),
                at: rhs.location(4),
            },
            "stmt_while" | "stmt_loop" => {
                let label = rhs.take(1).name();
                let (index, condition) = if lhs == "stmt_while" {
                    (4, Some(rhs.take(3).expr()))
                } else {
                    (3, None)
                };
                let Reduced::LoopBody { body, end, at } = rhs.take(index) else {
                    unreachable!()
                };
                let lineno = s.line(rhs.location(2));
                if let Some(end) = end {
                    check_label(label.as_deref(), &end, at, s.source())?;
                }
                self.env.pop();
                Reduced::Statement(Statement::Loop {
                    lineno,
                    label,
                    condition,
                    body,
                })
            }
            "exit_type" => rhs.take(1),
            "stmt_exit" => self.exit(rhs, s)?,
            "stmt_assert" => {
                let lineno = s.line(rhs.location(1));
                let (condition, end) = self.fragment(s, &[44, 59], ", or ;", expr)?;
                let message = if end == 44 {
                    Some(self.fragment(s, &[59], ";", expr)?.0)
                } else {
                    None
                };
                Reduced::Statement(Statement::Assert {
                    lineno,
                    condition,
                    message,
                })
            }
            "opt_transaction_chain" if prod == "K_AND K_CHAIN" => rhs.take(1),
            "opt_transaction_chain" => Reduced::None,
            "stmt_commit" | "stmt_rollback" => Reduced::Statement(Statement::Transaction {
                lineno: s.line(rhs.location(1)),
                commit: lhs == "stmt_commit",
                chain: matches!(rhs.value(2), Reduced::Token(_)),
            }),
            _ => return Err(CompileError::Unsupported { production: sig }),
        };
        Ok(result)
    }

    fn exit(&self, rhs: &mut Reduction<Reduced>, s: &BodyScanner) -> Result<Reduced, CompileError> {
        let lineno = s.line(rhs.location(1));
        let is_exit = rhs.take(1).token().id == token::K_EXIT;
        let label = rhs.take(2).name();
        let message = match (
            label.as_deref(),
            self.env.label_kind(label.as_deref()),
            is_exit,
        ) {
            (Some(l), None, _) => Some(format!(
                "there is no label \"{l}\" attached to any block or loop enclosing this statement"
            )),
            (Some(l), Some(false), false) => {
                Some(format!("block label \"{l}\" cannot be used in CONTINUE"))
            }
            (None, None, true) => {
                Some("EXIT cannot be used outside a loop, unless it has a label".into())
            }
            (None, None, false) => Some("CONTINUE cannot be used outside a loop".into()),
            _ => None,
        };
        if let Some(m) = message {
            let at = rhs.location(if label.is_some() { 2 } else { 1 });
            return Err(Error::at(s.source(), at, m, "42601").into());
        }
        let condition = match rhs.take(3) {
            Reduced::Expression(e) => Some(e),
            _ => None,
        };
        Ok(Reduced::Statement(Statement::Exit {
            lineno,
            is_exit,
            label,
            condition,
        }))
    }
}
