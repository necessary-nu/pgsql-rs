use crate::{protobuf, ParserOptions, Result, SummaryResult};
use protobuf::summary_result::{FilterColumn, Function, Table};

/// Parses the given SQL statement and provides a summary of it.
///
/// The native backend gathers table and function references from the raw tree.
/// A nonnegative limit also requests a shortened query. Pass -1 to omit it.
///
/// # Example
///
/// ```rust
/// use pgsql_syntax::{Node, NodeEnum, NodeRef};
///
/// let result = pgsql_syntax::summary("SELECT * FROM contacts", 0, -1);
/// assert!(result.is_ok());
/// let result = result.unwrap();
/// assert_eq!(result.tables(), vec!["contacts"]);
/// ```
pub fn summary(
    statement: &str,
    parser_options: impl Into<ParserOptions>,
    truncate_limit: i32,
) -> Result<SummaryResult> {
    crate::detailed::summary(statement, parser_options, truncate_limit).map_err(Into::into)
}

/// Compute fresh metadata from a caller-owned, possibly edited tree.
pub fn analyze(tree: &protobuf::ParseResult) -> Result<SummaryResult> {
    analyze_tree(tree).map(from_native)
}

pub(crate) fn analyze_tree(tree: &protobuf::ParseResult) -> Result<pgsql_parser::summary::Summary> {
    let statements = crate::bridge::statements(tree)?;
    Ok(pgsql_parser::summary::analyze_statements(&statements))
}

pub(crate) fn from_native(summary: pgsql_parser::summary::Summary) -> SummaryResult {
    let protobuf = protobuf::SummaryResult {
        tables: summary
            .tables
            .into_iter()
            .map(|t| Table {
                name: t.name,
                schema_name: t.schema_name,
                table_name: t.table_name,
                context: t.context,
            })
            .collect(),
        aliases: summary.aliases.into_iter().collect(),
        cte_names: summary.cte_names,
        functions: summary
            .functions
            .into_iter()
            .map(|f| Function {
                name: f.name,
                schema_name: f.schema_name,
                function_name: f.function_name,
                context: f.context,
            })
            .collect(),
        filter_columns: summary
            .filter_columns
            .into_iter()
            .map(|c| FilterColumn {
                schema_name: c.schema_name,
                table_name: c.table_name,
                column: c.column,
            })
            .collect(),
        truncated_query: summary.truncated_query.unwrap_or_default(),
        statement_types: summary.statement_types,
    };
    SummaryResult::new(protobuf, String::new())
}
