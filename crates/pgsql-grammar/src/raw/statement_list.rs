use super::statements::StatementSpanError;
use super::{Arena, Location, RawStmt, StatementList};
impl StatementList {
    pub fn terminate_last_at(&mut self, semicolon: Location) -> Result<(), StatementSpanError> {
        if let Some(mut stmt) = self.0.last() {
            stmt.terminate_at(semicolon)?;
            self.0.set(self.0.len() - 1, stmt);
        }
        Ok(())
    }

    pub fn slice<'a>(&self, arena: &'a Arena) -> &'a [RawStmt] {
        self.0.slice(arena)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
}
