use super::*;

impl<S: EmbeddedSql> Compiler<'_, S> {
    pub(super) fn execsql(
        &mut self,
        head: Token,
        s: &mut BodyScanner,
    ) -> Result<Reduced, CompileError> {
        if matches!(head.id, token::T_WORD | token::T_CWORD)
            && matches!(s.peek_raw(0)?.id, 61 | token::COLON_EQUALS | 91 | 46)
        {
            return Err(Error::at(
                s.source(),
                head.span.start,
                format!("\"{}\" is not a known variable", head.names.join(".")),
                "42601",
            )
            .into());
        }
        let saved = std::mem::replace(&mut s.lookup, Lookup::Expression);
        let result = self.execsql_text(&head, s);
        s.lookup = saved;
        let (query, target) = result?;
        self.sql
            .validate(&query, ParseMode::Sql)
            .map_err(|e| e.in_body(s.source(), head.span.start))?;
        Ok(Reduced::Statement(Statement::ExecSql {
            target,
            lineno: s.line(head.span.start),
            expression: Expression {
                query,
                mode: ParseMode::Sql,
            },
        }))
    }

    fn execsql_text(
        &mut self,
        head: &Token,
        s: &mut BodyScanner,
    ) -> Result<(String, Option<Box<crate::Target>>), CompileError> {
        let mut target = None;
        let mut redaction = None;
        let mut previous = head.id;
        let mut parens = 0usize;
        let mut blocks = 0usize;
        let mut routine = false;
        let mut prefix = if head
            .names
            .first()
            .is_some_and(|n| head.id == token::T_WORD && n == "create")
        {
            1
        } else {
            0
        };
        loop {
            let t = s.next_token(self)?;
            prefix = match (prefix, t.id, t.names.first().map(String::as_str)) {
                (1, token::K_OR, _) => 2,
                (2, token::T_WORD, Some("replace")) => 3,
                (1 | 3, token::T_WORD, Some("function" | "procedure")) => {
                    routine = true;
                    0
                }
                _ => 0,
            };
            match t.id {
                40 => parens += 1,
                41 => parens = parens.saturating_sub(1),
                token::K_BEGIN | token::K_CASE if routine && parens == 0 => blocks += 1,
                token::K_END if routine && parens == 0 => blocks = blocks.saturating_sub(1),
                59 if parens == 0 && blocks == 0 => {
                    let mut query = s.source()[head.span.start..t.span.start].to_owned();
                    if let Some((start, end)) = redaction {
                        query.replace_range(
                            start - head.span.start..end - head.span.start,
                            &" ".repeat(end - start),
                        );
                    }
                    query.truncate(
                        query
                            .trim_end_matches([' ', '\t', '\x0b', '\x0c', '\r', '\n'])
                            .len(),
                    );
                    return Ok((query, target));
                }
                0 => return Err(s.error("unexpected end of function definition").into()),
                token::K_INTO
                    if !matches!(previous, token::K_INSERT | token::K_MERGE)
                        && head.id != token::K_IMPORT =>
                {
                    if target.is_some() {
                        return Err(s.error("INTO specified more than once").into());
                    }
                    s.lookup = Lookup::Normal;
                    target = Some(Box::new(self.read_into_target(s)?));
                    s.lookup = Lookup::Expression;
                    redaction = Some((t.span.start, s.peek_raw(0)?.span.start));
                }
                _ => {}
            }
            previous = t.id;
        }
    }
}
