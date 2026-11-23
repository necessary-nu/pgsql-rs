use crate::{
    protobuf, DeparseComment, DeparseOptions, Error, FingerprintOptions, ParseResult,
    ParserOptions, Result,
};
use pgsql_parser as native;

/// Represents the resulting fingerprint containing both the raw integer form as well as the
/// corresponding 16 character hex value.
pub struct Fingerprint {
    pub value: u64,
    pub hex: String,
}

pub(crate) fn check_input(sql: &str) -> Result<()> {
    if sql.as_bytes().contains(&0) {
        std::ffi::CString::new(sql)?;
    }
    Ok(())
}

impl ParserOptions {
    pub(crate) fn native(self) -> (native::ParseMode, native::Options) {
        use native::ParseMode::*;
        let mode = match self.bits() & 15 {
            1 => TypeName,
            2 => PlPgSqlExpr,
            3 => PlPgSqlAssign1,
            4 => PlPgSqlAssign2,
            5 => PlPgSqlAssign3,
            _ => Sql,
        };
        (
            mode,
            native::Options {
                disable_backslash_quote: self.bits() & Self::DISABLE_BACKSLASH_QUOTE.bits() != 0,
            },
        )
    }
}

/// Parses the given SQL statement into the given abstract syntax tree.
///
/// The `parser_options` control how the statement is parsed: pass a
/// [`ParserOptions`] constant, a raw integer, or `0` for the standard
/// top-level SQL grammar.
///
/// # Example
///
/// ```rust
/// use pgsql_syntax::{Node, NodeEnum, NodeRef};
///
/// let result = pgsql_syntax::parse("SELECT * FROM contacts", 0);
/// assert!(result.is_ok());
/// let result = result.unwrap();
/// assert_eq!(result.tables(), vec!["contacts"]);
/// assert!(matches!(result.protobuf.nodes()[0].0, NodeRef::SelectStmt(_)));
/// ```
pub fn parse(statement: &str, parser_options: impl Into<ParserOptions>) -> Result<ParseResult> {
    crate::detailed::parse(statement, parser_options).map_err(Into::into)
}

/// Converts a parsed tree back into a string.
///
/// `options` accepts a [`DeparseOptions`] struct; pass `Default::default()`
/// for the plain, unformatted output.
///
/// # Example
///
/// ```rust
/// use pgsql_syntax::{Node, NodeEnum, NodeRef};
///
/// let result = pgsql_syntax::parse("INSERT INTO other (name) SELECT name FROM contacts", 0);
/// let result = result.unwrap();
/// let insert = result.protobuf.nodes()[0].0;
/// let select = result.protobuf.nodes()[1].0;
/// assert!(matches!(insert, NodeRef::InsertStmt(_)));
/// assert!(matches!(select, NodeRef::SelectStmt(_)));
///
/// // The entire parse result can be deparsed:
/// assert_eq!(result.deparse(Default::default()).unwrap(), "INSERT INTO other (name) SELECT name FROM contacts");
/// // Or an individual node can be deparsed:
/// assert_eq!(insert.deparse(Default::default()).unwrap(), "INSERT INTO other (name) SELECT name FROM contacts");
/// assert_eq!(select.deparse(Default::default()).unwrap(), "SELECT name FROM contacts");
/// ```
///
/// Returns an error if a node cannot be rendered as a statement.
pub fn deparse(tree: &protobuf::ParseResult, options: DeparseOptions) -> Result<String> {
    let statements = crate::bridge::statements(tree)?;
    deparse_statements(&statements, options)
}

pub(crate) fn deparse_statements(
    stmts: &[native::nodes::RawStmt],
    options: DeparseOptions,
) -> Result<String> {
    let comments = options
        .comments
        .iter()
        .map(|c| {
            check_input(&c.text)?;
            Ok(native::deparse::Comment {
                text: &c.text,
                match_location: c.match_location.max(0) as usize,
                newlines_before: c.newlines_before_comment.max(0) as usize,
                newlines_after: c.newlines_after_comment.max(0) as usize,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let opts = native::deparse::Options {
        pretty_print: options.pretty_print,
        trailing_newline: options.trailing_newline,
        commas_start_of_line: options.commas_start_of_line,
        indent_size: options.indent_size.max(0) as usize,
        max_line_length: options.max_line_length.max(0) as usize,
    };
    native::deparse::statements_with_options(stmts, opts, &comments)
        .map_err(|e| Error::Parse(e.to_string()))
}

/// Extract the comments from a query, along with the metadata needed to
/// re-insert them into deparsed output via [`DeparseOptions::comments`].
///
/// # Example
///
/// ```rust
/// let query = "SELECT 1 -- cast to string\n";
/// let comments = pgsql_syntax::deparse_comments_for_query(query).unwrap();
/// assert_eq!(comments.len(), 1);
/// assert_eq!(comments[0].text, "-- cast to string");
///
/// let parsed = pgsql_syntax::parse(query, 0).unwrap();
/// let output = pgsql_syntax::deparse(
///     &parsed.protobuf,
///     pgsql_syntax::DeparseOptions {
///         comments,
///         pretty_print: true,
///         ..Default::default()
///     },
/// )
/// .unwrap();
/// assert!(output.contains("-- cast to string"));
/// ```
pub fn deparse_comments_for_query(query: &str) -> Result<Vec<DeparseComment>> {
    crate::detailed::deparse_comments_for_query(query).map_err(Into::into)
}

/// Normalizes the given SQL statement, returning a parameterized version.
///
/// # Example
///
/// ```rust
/// let result = pgsql_syntax::normalize("SELECT * FROM contacts WHERE name='Paul'");
/// assert!(result.is_ok());
/// let result = result.unwrap();
/// assert_eq!(result, "SELECT * FROM contacts WHERE name=$1");
/// ```
pub fn normalize(statement: &str) -> Result<String> {
    crate::detailed::normalize(statement).map_err(Into::into)
}

/// Fingerprints the given SQL statement. Useful for comparing parse trees across different implementations
/// of `libpg_query`.
///
/// `parser_options` and `fingerprint_options` accept [`ParserOptions`] /
/// [`FingerprintOptions`] constants or raw integers; pass `0` to use the
/// defaults for either.
///
/// Fingerprinting is a superset of the Postgres `queryid` mechanism: for a
/// given `queryid` there is only one valid fingerprint (e.g. constants are
/// ignored, so `WHERE id = 123` and `WHERE id = 456` fingerprint the same).
///
/// Fingerprints follow Postgres 18 query ID behavior: in SELECT/DML
/// statements a relation alias (if present) replaces the relation name and
/// schema names are ignored, while sequences of 2+ digits in relation names
/// are ignored by default. Pass [`FingerprintOptions::RANGEVAR_PG17_COMPAT`]
/// for Postgres 17 compatible fingerprints, or use the other
/// [`FingerprintOptions`] flags to tune relation handling.
///
/// # Example
///
/// ```rust
/// let result = pgsql_syntax::fingerprint("SELECT * FROM contacts WHERE name='Paul'", 0, 0);
/// assert!(result.is_ok());
/// let result = result.unwrap();
/// assert_eq!(result.hex, "0e2581a461ece536");
/// ```
///
/// # Example: with fingerprint options
///
/// ```rust
/// // By default the full relation name is not fingerprinted, so both match...
/// let default = pgsql_syntax::fingerprint("SELECT * FROM orders_2024_01", 0, 0).unwrap();
/// assert_eq!(default.hex, pgsql_syntax::fingerprint("SELECT * FROM orders_2024_02", 0, 0).unwrap().hex);
///
/// // ...unless PG_QUERY_FINGERPRINT_FULL_RELNAME is set
/// let result = pgsql_syntax::fingerprint("SELECT * FROM orders_2024_01", 0, pgsql_syntax::FingerprintOptions::FULL_RELNAME).unwrap();
/// assert_eq!(result.hex, "3cc2d1ca3f22c9bf");
/// ```
pub fn fingerprint(
    statement: &str,
    parser_options: impl Into<ParserOptions>,
    fingerprint_options: impl Into<FingerprintOptions>,
) -> Result<Fingerprint> {
    crate::detailed::fingerprint(statement, parser_options, fingerprint_options).map_err(Into::into)
}

/// An experimental API which parses a PLPGSQL function. This currently returns the raw JSON structure.
///
/// # Example
///
/// ```rust
/// let result = pgsql_syntax::parse_plpgsql("
///     CREATE OR REPLACE FUNCTION cs_fmt_browser_version(v_name varchar, v_version varchar)
///     RETURNS varchar AS $$
///     BEGIN
///         IF v_version IS NULL THEN
///             RETURN v_name;
///         END IF;
///         RETURN v_name || '/' || v_version;
///     END;
///     $$ LANGUAGE plpgsql;
/// ");
/// assert!(result.is_ok());
/// ```
pub fn parse_plpgsql(stmt: &str) -> Result<serde_json::Value> {
    crate::detailed::parse_plpgsql(stmt).map_err(Into::into)
}

/// Split a well-formed query into separate statements.
///
/// # Example
///
/// ```rust
/// let query = r#"select /*;*/ 1; select "2;", (select 3);"#;
/// let statements = pgsql_syntax::split_with_parser(query).unwrap();
/// assert_eq!(statements, vec!["select /*;*/ 1", r#"select "2;", (select 3)"#]);
/// ```
///
/// However, `split_with_parser` will fail on malformed statements
///
/// ```rust
/// let query = "select 1; this statement is not sql; select 2;";
/// let result = pgsql_syntax::split_with_parser(query);
/// let err = r#"syntax error at or near "this""#;
/// assert_eq!(result, Err(pgsql_syntax::Error::Split(err.to_string())));
/// ```
pub fn split_with_parser(query: &str) -> Result<Vec<&str>> {
    crate::detailed::split_with_parser(query).map_err(Into::into)
}

/// Split a potentially-malformed query into separate statements. Note that
/// invalid tokens will be skipped
/// ```rust
/// let query = r#"select /*;*/ 1; asdf; select "2;", (select 3); asdf"#;
/// let statements = pgsql_syntax::split_with_scanner(query).unwrap();
/// assert_eq!(statements, vec![
///     "select /*;*/ 1",
///     // skipped " asdf" since it was an invalid token
///     r#" select "2;", (select 3)"#,
/// ]);
/// ```
pub fn split_with_scanner(query: &str) -> Result<Vec<&str>> {
    crate::detailed::split_with_scanner(query).map_err(Into::into)
}

/// Scan SQL into a versioned sequence of PostgreSQL tokens.
///
/// # Example
///
/// ```rust
/// use pgsql_syntax::protobuf::*;
/// let sql = "SELECT update AS left /* comment */ FROM between";
/// let result = pgsql_syntax::scan(sql).unwrap();
/// let tokens: Vec<std::string::String> = result.tokens.iter().map(|token| {
///     format!("{:?}", token)
/// }).collect();
/// assert_eq!(
///     tokens,
///     vec![
///         "ScanToken { start: 0, end: 6, token: Select, keyword_kind: ReservedKeyword }",
///         "ScanToken { start: 7, end: 13, token: Update, keyword_kind: UnreservedKeyword }",
///         "ScanToken { start: 14, end: 16, token: As, keyword_kind: ReservedKeyword }",
///         "ScanToken { start: 17, end: 21, token: Left, keyword_kind: TypeFuncNameKeyword }",
///         "ScanToken { start: 22, end: 35, token: CComment, keyword_kind: NoKeyword }",
///         "ScanToken { start: 36, end: 40, token: From, keyword_kind: ReservedKeyword }",
///         "ScanToken { start: 41, end: 48, token: Between, keyword_kind: ColNameKeyword }"
///     ]);
/// ```
pub fn scan(sql: &str) -> Result<protobuf::ScanResult> {
    crate::detailed::scan(sql).map_err(Into::into)
}

pub(crate) fn scan_offset(n: usize) -> Result<i32> {
    i32::try_from(n)
        .map_err(|_| Error::Scan("source offset exceeds PostgreSQL's int32 range".into()))
}

/// Return the native scanner's tokens with PostgreSQL byte offsets and keyword
/// categories. The result equals `scan(sql)?.tokens`.
///
/// # Example
///
/// ```rust
/// let sql = "SELECT update AS left /* comment */ FROM between";
/// let tokens = pgsql_syntax::scan_tokens(sql).unwrap();
/// assert_eq!(tokens, pgsql_syntax::scan(sql).unwrap().tokens);
/// ```
pub fn scan_tokens(sql: &str) -> Result<Vec<protobuf::ScanToken>> {
    crate::detailed::scan_tokens(sql).map_err(Into::into)
}

/// Determine whether each statement in a query is a utility statement.
///
/// Statements such as `SHOW`, `SET`, and `CREATE TABLE` are utility statements.
/// For multiple statements, results follow their input order.
///
/// # Example
///
/// ```rust
/// let result = pgsql_syntax::is_utility_stmt("SELECT 1; SHOW fsync;").unwrap();
/// assert_eq!(result, vec![false, true]);
/// ```
pub fn is_utility_stmt(query: &str) -> Result<Vec<bool>> {
    crate::detailed::is_utility_stmt(query).map_err(Into::into)
}
