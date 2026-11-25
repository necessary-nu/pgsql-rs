use super::{Location, RawStmt, StatementList, StatementSpanError};
impl StatementList {
    pub fn terminate_last_at(&mut self, semicolon: Location) -> Result<(), StatementSpanError> {
        if let Some(stmt) = self.0.last_mut() {
            stmt.terminate_at(semicolon)?;
        }
        Ok(())
    }

    pub fn as_slice(&self) -> &[RawStmt] {
        &self.0
    }

    pub fn into_statements(self) -> Vec<RawStmt> {
        self.0
    }
}
