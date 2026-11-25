use std::ops::Range;

use super::{Box, Location, Node, RawStmt, Vec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatementSpanError {
    UnknownOffset,
    EndBeforeStart,
}

impl RawStmt {
    #[inline]
    pub fn new(stmt: Node, start: Location) -> Self {
        Self {
            stmt: Some(Box::new(stmt)),
            stmt_location: start,
            stmt_len: 0,
        }
    }

    pub fn terminate_at(&mut self, semicolon: Location) -> Result<(), StatementSpanError> {
        if self.stmt_len > 0 {
            return Ok(());
        }
        let start = self
            .stmt_location
            .byte_offset()
            .ok_or(StatementSpanError::UnknownOffset)?;
        let end = semicolon
            .byte_offset()
            .ok_or(StatementSpanError::UnknownOffset)?;
        if end < start {
            return Err(StatementSpanError::EndBeforeStart);
        }
        self.stmt_len = semicolon.as_pg() - self.stmt_location.as_pg();
        Ok(())
    }

    pub fn source_range(&self, source_len: usize) -> Option<Range<usize>> {
        let start = self.stmt_location.byte_offset()?;
        let end = if self.stmt_len == 0 {
            source_len
        } else {
            start.checked_add(usize::try_from(self.stmt_len).ok()?)?
        };
        (start <= end && end <= source_len).then_some(start..end)
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct StatementList(pub(super) Vec<RawStmt>);

// [spec:pgq:req:tree.statement-spans]
impl StatementList {
    #[inline]
    pub fn from_first(statement: Option<Node>, start: Location) -> Self {
        Self(match statement {
            Some(stmt) => vec![RawStmt::new(stmt, start)],
            None => Vec::new(),
        })
    }

    pub fn push(&mut self, statement: Option<Node>, start: Location) {
        if let Some(stmt) = statement {
            self.0.push(RawStmt::new(stmt, start));
        }
    }
}
