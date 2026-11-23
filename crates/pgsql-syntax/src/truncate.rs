use crate::{protobuf, Error, Result};

/// Shorten deparsed SQL by eliding clauses, then truncating at a character boundary.
pub fn truncate(tree: &protobuf::ParseResult, max_length: usize) -> Result<String> {
    let stmts = crate::bridge::statements(tree)?;
    pgsql_parser::summary::truncate_statements(stmts, max_length)
        .map_err(|e| Error::Parse(e.to_string()))
}
