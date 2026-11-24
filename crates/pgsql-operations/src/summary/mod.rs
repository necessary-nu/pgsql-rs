//! Query entities and statement types from the PostgreSQL raw tree.
mod entities;
mod scoped;
pub(crate) mod tree;
pub use scoped::analyze_statements;
mod model;
mod truncate;
use crate::raw_walk::{is_summary_statement, summary_walks_children, RawRef};
pub use model::*;
use pgsql_grammar::{parse, Error, Options, ParseMode};
use pgsql_nodes::*;
use prost::Message;
pub use truncate::{truncate_statements, truncate_summary_statements};

/// Match the C summary wire API. Use [analyze_statements] for query-block-aware metadata.
pub fn summarize(sql: &str, mode: ParseMode, options: Options) -> Result<Summary, Error> {
    let nodes = parse(sql, mode, options)?;
    Ok(summarize_nodes(&nodes))
}

pub fn summary_protobuf(sql: &str, mode: ParseMode, options: Options) -> Result<Vec<u8>, Error> {
    let result = summarize(sql, mode, options)?;
    Ok(result.encode_to_vec())
}

#[derive(Debug, PartialEq, Eq)]
pub enum SummaryError {
    Parse(Error),
    Deparse(crate::deparse::Error),
}

impl std::fmt::Display for SummaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => e.fmt(f),
            Self::Deparse(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for SummaryError {}

pub fn summarize_with_limit(
    sql: &str,
    mode: ParseMode,
    options: Options,
    limit: i32,
) -> Result<Summary, SummaryError> {
    let nodes = parse(sql, mode, options).map_err(SummaryError::Parse)?;
    let mut summary = summarize_nodes(&nodes);
    if limit != -1 {
        summary.truncated_query =
            Some(truncate::truncate(nodes, limit).map_err(SummaryError::Deparse)?);
    }
    Ok(summary)
}

// [spec:pgq:req:summary.protobuf]
pub fn summary_protobuf_with_limit(
    sql: &str,
    mode: ParseMode,
    options: Options,
    limit: i32,
) -> Result<Vec<u8>, SummaryError> {
    let summary = summarize_with_limit(sql, mode, options, limit)?;
    Ok(summary.encode_to_vec())
}

// [spec:pgq:req:summary.entities]
pub fn summarize_nodes(nodes: &[Node]) -> Summary {
    let mut state = entities::State::default();
    state.walk(nodes);
    let mut summary = state.finish();
    let mut jobs = vec![RawRef::List(nodes)];
    while let Some(r) = jobs.pop() {
        if let Some(tag) = r.tag().filter(|&tag| is_summary_statement(tag)) {
            let name = tag.pg_name();
            if !summary.statement_types.iter().any(|s| s == name) {
                summary.statement_types.push(name.to_owned());
            }
        }
        let query = match r {
            RawRef::CreateTableAsStmt(s) => s.query.as_deref(),
            RawRef::ExplainStmt(s) => s.query.as_deref(),
            RawRef::PrepareStmt(s) => s.query.as_deref(),
            RawRef::ViewStmt(s) => s.query.as_deref(),
            RawRef::RawStmt(s) => s.stmt.as_deref(),
            _ => None,
        };
        children(r, &mut jobs);
        if let Some(query) = query {
            jobs.push(query.into());
        }
    }
    summary
}

fn children<'a>(r: RawRef<'a>, jobs: &mut Vec<RawRef<'a>>) {
    if matches!(r, RawRef::List(_)) || r.tag().is_some_and(summary_walks_children) {
        let start = jobs.len();
        r.children(&mut |n| jobs.push(n));
        jobs[start..].reverse();
    }
}
