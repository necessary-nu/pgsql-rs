use pg19_engine::ParseMode;
use pg19_lexer::{BackslashQuote, Error, Options, Scanner};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn error(e: Error) -> Value {
    json!({"sqlstate":e.sqlstate, "message":e.message, "position":e.position})
}

// [spec:pgq:req:grammar.corpus-parity]
fn probe(sql: &str, mode: &str, backslash_quote: &str) -> Result<Value> {
    let mode = match mode {
        "default" => ParseMode::Sql,
        "type-name" => ParseMode::TypeName,
        "plpgsql-expr" => ParseMode::PlPgSqlExpr,
        "plpgsql-assign1" => ParseMode::PlPgSqlAssign1,
        "plpgsql-assign2" => ParseMode::PlPgSqlAssign2,
        "plpgsql-assign3" => ParseMode::PlPgSqlAssign3,
        _ => return Err(format!("unknown parse mode {mode}").into()),
    };
    let options = Options::with_backslash_quote(match backslash_quote {
        "off" => BackslashQuote::Off,
        "on" => BackslashQuote::On,
        "safe" | "safe_encoding" => BackslashQuote::SafeEncoding,
        _ => return Err(format!("unknown backslash quote setting {backslash_quote}").into()),
    });
    let mut tokens = Vec::new();
    let mut scan_error = Value::Null;
    for token in Scanner::with_options(sql, options) {
        match token {
            Ok(t) => tokens.push(json!({
                "id":t.kind.id(), "name":t.kind.name(), "start":t.start, "end":t.end
            })),
            Err(e) => {
                scan_error = error(e);
                break;
            }
        }
    }
    let mut out =
        json!({"tree":null, "parse_error":null, "tokens":tokens, "scan_error":scan_error});
    match pg19_parser::parse(sql, mode, options) {
        Ok(nodes) => out["tree"] = pg19_reference::nodes_text(&nodes)?.into(),
        Err(pg19_parser::Error::Diagnostic(e)) => out["parse_error"] = error(e),
        Err(pg19_parser::Error::Unmapped(rule)) => {
            out["missing_action"] = json!({"production":rule.production, "line":rule.line,
                "sha256":rule.action.map(|action| action.sha256)});
        }
        Err(e) => out["action_error"] = e.to_string().into(),
    }
    Ok(out)
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    if args == ["--batch"] {
        for line in io::stdin().lock().lines() {
            let case: Value = serde_json::from_str(&line?)?;
            let id = case["id"].as_str().ok_or("missing case id")?;
            let sql = case["sql"].as_str().ok_or("missing SQL")?;
            let mode = case["mode"].as_str().ok_or("missing mode")?;
            let quote = case["backslash_quote"]
                .as_str()
                .ok_or("missing backslash_quote")?;
            let result = probe(sql, mode, quote).map_err(|e| format!("{id}: {e}"))?;
            serde_json::to_writer(&mut stdout, &json!({"id":id, "result":result}))?;
            writeln!(stdout)?;
        }
    } else {
        let (sql, opts) = args.split_first().ok_or("expected SQL or --batch")?;
        let (mut mode, mut quote) = ("default", "safe_encoding");
        for pair in opts.chunks(2) {
            match pair {
                [key, val] if key == "--mode" => mode = val,
                [key, val] if key == "--backslash-quote" => quote = val,
                _ => return Err(format!("invalid probe options {pair:?}").into()),
            }
        }
        serde_json::to_writer(&mut stdout, &probe(sql, mode, quote)?)?;
        writeln!(stdout)?;
    }
    stdout.flush()?;
    Ok(())
}
