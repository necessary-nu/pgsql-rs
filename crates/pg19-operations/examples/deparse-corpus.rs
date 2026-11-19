//! Render default-mode regression cases for the PG19 reference comparison.
use serde_json::{json, Value};
use std::io::{self, BufRead, BufWriter, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut out = BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let case: Value = serde_json::from_str(&line?)?;
        if case["mode"] != "default" {
            continue;
        }
        let sql = case["sql"].as_str().ok_or("missing SQL")?;
        let backslash_quote = match case["backslash_quote"].as_str() {
            Some("off") => pg19_lexer::BackslashQuote::Off,
            Some("on") => pg19_lexer::BackslashQuote::On,
            Some("safe_encoding") => pg19_lexer::BackslashQuote::SafeEncoding,
            _ => return Err("unknown backslash_quote setting".into()),
        };
        let options = pg19_lexer::Options::with_backslash_quote(backslash_quote);
        let result = match pg19_parser::parse_statements(sql, options) {
            Err(_) => json!({"id":case["id"],"parse_error":true}),
            Ok(tree) => match pg19_operations::deparse::statements(&tree) {
                Ok(rendered) => json!({"id":case["id"],"sql":sql,"rendered":rendered}),
                Err(error) => json!({"id":case["id"],"deparse_error":error.to_string()}),
            },
        };
        serde_json::to_writer(&mut out, &result)?;
        out.write_all(b"\n")?;
    }
    Ok(())
}
