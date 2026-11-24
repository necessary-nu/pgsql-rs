use pgsql_lexer::{Error, Scanner};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment<'sql> {
    pub text: &'sql str,
    /// End byte of the preceding non-comment token, or zero for leading comments.
    pub match_location: usize,
    pub newlines_before: usize,
    pub newlines_after: usize,
}

pub fn comments(sql: &str) -> Result<Vec<Comment<'_>>, Error> {
    let mut comments: Vec<Comment<'_>> = Vec::new();
    let mut prior_end = 0;
    let mut match_location = 0;
    let mut prior_was_comment = false;
    for token in Scanner::new(sql) {
        let token = token?;
        let is_comment = token.kind.is_comment();
        if is_comment || prior_was_comment {
            let lines = sql.as_bytes()[prior_end..token.start]
                .iter()
                .filter(|&&byte| byte == b'\n')
                .count();
            if prior_was_comment {
                comments
                    .last_mut()
                    .expect("preceding comment")
                    .newlines_after = lines;
            }
            if is_comment {
                comments.push(Comment {
                    text: &sql[token.start..token.end],
                    match_location,
                    newlines_before: if prior_was_comment { 0 } else { lines },
                    newlines_after: 0,
                });
            }
        }
        if !is_comment {
            match_location = token.end;
        }
        prior_was_comment = is_comment;
        prior_end = token.end;
    }
    Ok(comments)
}
