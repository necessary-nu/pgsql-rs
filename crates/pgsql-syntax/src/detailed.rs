//! SQL operations with the original PostgreSQL diagnostic, including its SQLSTATE
//! and one-based character cursor. PL/pgSQL body errors may also include context.
use crate::{
    protobuf,
    query::{check_input, scan_offset},
    DeparseComment, Fingerprint, FingerprintOptions, ParseResult, ParserOptions, SummaryResult,
    PG_VERSION_NUM,
};
use pgsql_parser as native;

pub use native::lexer::Error as Diagnostic;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Parse,
    Scan,
    Split,
    IsUtility,
}

/// Source text to which the diagnostic cursor refers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Sql,
    RoutineBody,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("{diagnostic}")]
    Postgres {
        operation: Operation,
        #[source]
        diagnostic: Diagnostic,
        context: Option<String>,
        source_text: Source,
    },
    #[error(transparent)]
    Api(#[from] crate::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Operation {
    fn legacy(self, message: String) -> crate::Error {
        match self {
            Self::Parse => crate::Error::Parse(message),
            Self::Scan => crate::Error::Scan(message),
            Self::Split => crate::Error::Split(message),
            Self::IsUtility => crate::Error::IsUtility(message),
        }
    }

    fn diagnostic(self, diagnostic: Diagnostic) -> Error {
        Error::Postgres {
            operation: self,
            diagnostic,
            context: None,
            source_text: Source::Sql,
        }
    }

    fn parser(self, error: native::ParseError) -> Error {
        match error {
            native::ParseError::Diagnostic(diagnostic) => self.diagnostic(diagnostic),
            other => self.legacy(other.to_string()).into(),
        }
    }
}

impl From<Error> for crate::Error {
    fn from(error: Error) -> Self {
        match error {
            Error::Api(e) => e,
            Error::Postgres {
                operation,
                diagnostic,
                ..
            } => operation.legacy(diagnostic.message),
        }
    }
}

fn statements(sql: &str, options: ParserOptions) -> Result<Vec<native::nodes::RawStmt>> {
    check_input(sql)?;
    let (mode, opts) = options.native();
    if mode == native::ParseMode::Sql {
        return native::parse_statements(sql, opts).map_err(|e| Operation::Parse.parser(e));
    }
    let nodes = native::parse(sql, mode, opts).map_err(|e| Operation::Parse.parser(e))?;
    Ok(nodes
        .into_iter()
        .map(|n| match n {
            native::nodes::Node::RawStmt(raw) => raw.into_inner(),
            other => native::nodes::RawStmt::new(other, Default::default()),
        })
        .collect())
}

/// Parse an SQL tree and snapshot its metadata.
pub fn parse(statement: &str, parser_options: impl Into<ParserOptions>) -> Result<ParseResult> {
    let statements = statements(statement, parser_options.into())?;
    let tree = crate::bridge::parse_tree(&statements)?;
    let summary = native::summary::analyze_statements(&statements);
    Ok(ParseResult::from_summary(tree, "", summary))
}

/// Analyze SQL with optional summary-style truncation.
pub fn summary(
    statement: &str,
    parser_options: impl Into<ParserOptions>,
    truncate_limit: i32,
) -> Result<SummaryResult> {
    let statements = statements(statement, parser_options.into())?;
    let mut summary = native::summary::analyze_statements(&statements);
    if truncate_limit != -1 {
        summary.truncated_query = Some(
            native::summary::truncate_summary_statements(statements, truncate_limit)
                .map_err(|e| crate::Error::Parse(e.to_string()))?,
        );
    }
    Ok(crate::summary::from_native(summary))
}

/// The [crate::deparse_comments_for_query] operation with structured diagnostics.
pub fn deparse_comments_for_query(query: &str) -> Result<Vec<DeparseComment>> {
    check_input(query)?;
    native::deparse::comments(query)
        .map_err(|e| Operation::Scan.diagnostic(e))?
        .into_iter()
        .map(|c| {
            Ok(DeparseComment {
                match_location: scan_offset(c.match_location)?,
                newlines_before_comment: scan_offset(c.newlines_before)?,
                newlines_after_comment: scan_offset(c.newlines_after)?,
                text: c.text.to_owned(),
            })
        })
        .collect()
}

/// The [crate::normalize] operation with structured diagnostics.
pub fn normalize(statement: &str) -> Result<String> {
    check_input(statement)?;
    native::normalize(statement).map_err(|e| Operation::Parse.parser(e))
}

/// The [crate::fingerprint] operation with structured diagnostics.
pub fn fingerprint(
    statement: &str,
    parser_options: impl Into<ParserOptions>,
    fingerprint_options: impl Into<FingerprintOptions>,
) -> Result<Fingerprint> {
    check_input(statement)?;
    let (mode, opts) = parser_options.into().native();
    let bits = fingerprint_options.into().bits();
    let fp = native::fingerprint::fingerprint_sql(
        statement,
        mode,
        opts,
        native::fingerprint::FingerprintOptions {
            ignore_aliases: bits & FingerprintOptions::RANGEVAR_IGNORE_ALIASES.bits() != 0,
            include_schema: bits & FingerprintOptions::RANGEVAR_INCLUDE_SCHEMA.bits() != 0,
            full_relation_name: bits & FingerprintOptions::FULL_RELNAME.bits() != 0,
        },
    )
    .map_err(|e| Operation::Parse.parser(e))?;
    Ok(Fingerprint {
        value: fp.0,
        hex: fp.to_string(),
    })
}

/// The [crate::parse_plpgsql] operation with structured diagnostics.
pub fn parse_plpgsql(stmt: &str) -> Result<serde_json::Value> {
    check_input(stmt)?;
    let routines =
        native::parse_routines(stmt, Default::default()).map_err(|e| match e.diagnostic() {
            Some((diagnostic, context)) => Error::Postgres {
                operation: Operation::Parse,
                diagnostic: diagnostic.clone(),
                source_text: match &e {
                    native::RoutineError::OuterSql(_) => Source::Sql,
                    native::RoutineError::Body { .. } => Source::RoutineBody,
                },
                context: context.map(str::to_owned),
            },
            None => crate::Error::Parse(e.to_string()).into(),
        })?;
    Ok(serde_json::Value::Array(
        routines.iter().map(|routine| routine.json()).collect(),
    ))
}

/// The [crate::split_with_parser] operation with structured diagnostics.
pub fn split_with_parser(query: &str) -> Result<Vec<&str>> {
    check_input(query)?;
    Ok(native::split_with_parser(query)
        .map_err(|e| Operation::Split.parser(e))?
        .into_iter()
        .map(|r| &query[r])
        .collect())
}

/// The [crate::split_with_scanner] operation with structured diagnostics.
pub fn split_with_scanner(query: &str) -> Result<Vec<&str>> {
    check_input(query)?;
    Ok(native::split_with_scanner(query)
        .map_err(|e| Operation::Split.diagnostic(e))?
        .into_iter()
        .map(|r| &query[r])
        .collect())
}

/// The [crate::scan] operation with structured diagnostics.
pub fn scan(sql: &str) -> Result<protobuf::ScanResult> {
    Ok(protobuf::ScanResult {
        version: PG_VERSION_NUM,
        tokens: scan_tokens(sql)?,
    })
}

/// The [crate::scan_tokens] operation with structured diagnostics.
pub fn scan_tokens(sql: &str) -> Result<Vec<protobuf::ScanToken>> {
    use native::lexer::KeywordKind::*;
    check_input(sql)?;
    native::lexer::Scanner::new(sql)
        .map(|t| {
            let t = t.map_err(|e| Operation::Scan.diagnostic(e))?;
            Ok(protobuf::ScanToken {
                start: scan_offset(t.start)?,
                end: scan_offset(t.end)?,
                token: i32::from(t.kind.id()),
                keyword_kind: match t.keyword {
                    None => 0,
                    Some(Unreserved) => 1,
                    Some(ColumnName) => 2,
                    Some(TypeFunctionName) => 3,
                    Some(Reserved) => 4,
                },
            })
        })
        .collect()
}

/// The [crate::is_utility_stmt] operation with structured diagnostics.
pub fn is_utility_stmt(query: &str) -> Result<Vec<bool>> {
    check_input(query)?;
    native::utility_statements(query).map_err(|e| Operation::IsUtility.parser(e))
}
