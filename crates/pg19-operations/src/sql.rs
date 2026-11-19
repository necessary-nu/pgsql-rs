use pg19_nodes::Node;
use pg19_parser::{parse_statements, Error, Options};
use std::ops::Range;

// [spec:pgq:req:split.parser]
// [spec:pgq:req:split.boundaries]
pub fn split_with_parser(sql: &str) -> Result<Vec<Range<usize>>, Error> {
    let sql = sql.split_once('\0').map_or(sql, |(prefix, _)| prefix);
    parse_statements(sql, Options::default())?
        .iter()
        .map(|stmt| {
            stmt.source_range(sql.len())
                .ok_or(Error::InvalidOutput("invalid raw statement span"))
        })
        .collect()
}

pub fn is_utility_statement(node: &Node) -> bool {
    !matches!(
        node,
        Node::SelectStmt(_)
            | Node::InsertStmt(_)
            | Node::UpdateStmt(_)
            | Node::DeleteStmt(_)
            | Node::MergeStmt(_)
    )
}

pub fn utility_statements(sql: &str) -> Result<Vec<bool>, Error> {
    parse_statements(sql, Options::default())?
        .iter()
        .map(|stmt| {
            stmt.stmt
                .as_deref()
                .map(is_utility_statement)
                .ok_or(Error::InvalidOutput("empty raw statement"))
        })
        .collect()
}
