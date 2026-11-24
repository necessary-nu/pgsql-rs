use super::*;
use crate::FetchDirection;

impl<S: EmbeddedSql> Compiler<'_, S> {
    pub(super) fn fetch_direction(
        &mut self,
        s: &mut BodyScanner,
    ) -> Result<FetchDirection, CompileError> {
        let mut dir = FetchDirection {
            direction: 0,
            count: 1,
            expression: None,
            multiple: false,
        };
        let mut t = s.next_token(self)?;
        let needs_count = if t.is_keyword(token::K_FORWARD, "forward")
            || t.is_keyword(token::K_BACKWARD, "backward")
        {
            dir.direction = t.is_keyword(token::K_BACKWARD, "backward") as u8;
            t = s.next_token(self)?;
            if t.id == token::K_ALL {
                dir.count = i64::MAX;
                dir.multiple = true;
                false
            } else if matches!(t.id, token::K_FROM | token::K_IN) {
                return Ok(dir);
            } else {
                dir.multiple = true;
                true
            }
        } else if t.is_keyword(token::K_ABSOLUTE, "absolute")
            || t.is_keyword(token::K_RELATIVE, "relative")
        {
            dir.direction = if t.is_keyword(token::K_ABSOLUTE, "absolute") {
                2
            } else {
                3
            };
            t = s.next_token(self)?;
            true
        } else if t.is_keyword(token::K_ALL, "all") {
            dir.count = i64::MAX;
            dir.multiple = true;
            false
        } else if t.is_keyword(token::K_FIRST, "first") || t.is_keyword(token::K_LAST, "last") {
            dir.direction = 2;
            if t.is_keyword(token::K_LAST, "last") {
                dir.count = -1;
            }
            false
        } else if t.is_keyword(token::K_PRIOR, "prior") {
            dir.direction = 1;
            false
        } else if t.is_keyword(token::K_NEXT, "next") {
            false
        } else if matches!(t.id, token::K_FROM | token::K_IN) {
            return Ok(dir);
        } else if t.id == token::T_DATUM {
            s.push_back(t);
            return Ok(dir);
        } else {
            dir.multiple = true;
            true
        };
        if t.id == 0 {
            return Err(s.error("unexpected end of function definition").into());
        }
        if needs_count {
            s.push_back(t);
            dir.expression = Some(
                self.fragment(
                    s,
                    &[token::K_FROM, token::K_IN],
                    "FROM or IN",
                    ParseMode::PlPgSqlExpr,
                )?
                .0,
            );
        } else if !matches!(s.next_token(self)?.id, token::K_FROM | token::K_IN) {
            return Err(s.error("expected FROM or IN").into());
        }
        Ok(dir)
    }

    pub(super) fn fetch(
        &mut self,
        lhs: &str,
        rhs: &mut Reduction<Reduced>,
        s: &mut BodyScanner,
    ) -> Result<Reduced, CompileError> {
        let Reduced::Fetch(direction) = rhs.take(2) else {
            unreachable!()
        };
        let cursor = rhs.take(3).datum();
        let at = rhs.location(1);
        let target = if lhs == "stmt_fetch" {
            let target = self.read_into_target(s, false)?;
            if s.next_token(self)?.id != 59 {
                return Err(s.error("syntax error").into());
            }
            if direction.multiple {
                return Err(Error::at(
                    s.source(),
                    at,
                    "FETCH statement cannot return multiple rows",
                    "0A000",
                )
                .into());
            }
            Some(Box::new(target))
        } else {
            None
        };
        Ok(Reduced::Statement(Statement::Fetch {
            lineno: s.line(at),
            cursor,
            target,
            direction,
        }))
    }
}
