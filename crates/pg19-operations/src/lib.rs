pub mod deparse;
pub mod fingerprint;
mod sql;
pub use sql::{is_utility_statement, split_with_parser, utility_statements};

use pg19_lexer::{Error, Scanner};
use std::ops::Range;

/// Return half-open byte ranges into the input. Each range includes leading
/// and trailing trivia and excludes its terminating semicolon.
// [spec:pgq:req:split.scanner]
pub fn split_with_scanner(sql: &str) -> Result<Vec<Range<usize>>, Error> {
    let mut scanner = Scanner::new(sql);
    let mut statements = Vec::new();
    let mut start = 0;
    let mut has_keyword = false;
    // The C splitter recognizes a boundary at net zero even if a closing
    // parenthesis preceded its opening partner in invalid SQL.
    let mut depth = 0isize;
    for token in scanner.by_ref() {
        let token = token?;
        if token.keyword.is_some() {
            has_keyword = true;
            continue;
        }
        match token.kind.name() {
            "(" => depth += 1,
            ")" => depth -= 1,
            ";" if depth == 0 => {
                if has_keyword {
                    statements.push(start..token.start);
                }
                start = token.end;
                has_keyword = false;
            }
            _ => {}
        }
    }
    if has_keyword && depth == 0 {
        statements.push(start..scanner.offset());
    }
    Ok(statements)
}
