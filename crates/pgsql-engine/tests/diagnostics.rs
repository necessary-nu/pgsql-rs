use pgsql_engine::{parse, Actions, ParseError, ParseMode, Reduction};
use pgsql_lexer::{BackslashQuote, Error, Kind, Options, Scanner, Token, Value};
use pgsql_tables::Production;
use serde_json::{json, Value as Json};
use std::{convert::Infallible, path::Path, process::Command, sync::Barrier, thread};

#[derive(Default)]
struct Literals(Vec<String>);

impl Actions for Literals {
    type Value = ();
    type Error = Infallible;

    fn shift(&mut self, token: Token) {
        if let (Kind::SCONST, Value::Text(s)) = (token.kind, token.value) {
            self.0.push(s);
        }
    }

    fn reduce(&mut self, _: Production, _: &mut Reduction<'_, ()>) -> Result<(), Infallible> {
        Ok(())
    }
}

fn run(sql: &str, mode: ParseMode, options: Options) -> Result<Vec<String>, Error> {
    let mut literals = Literals::default();
    match parse(sql, mode, options, &mut literals) {
        Ok(()) => Ok(literals.0),
        Err(ParseError::Diagnostic(e)) => Err(e),
        Err(ParseError::Action(never)) => match never {},
    }
}

fn cases() -> impl Iterator<Item = Json> {
    include_str!("diagnostics.jsonl")
        .lines()
        .map(|row| serde_json::from_str(row).unwrap())
}

fn diagnostic(error: Option<Error>) -> Json {
    error.map_or(
        Json::Null,
        |e| json!({"message": e.message, "sqlstate": e.sqlstate, "position": e.position}),
    )
}

fn mode(name: &str) -> ParseMode {
    match name {
        "default" => ParseMode::Sql,
        "type-name" => ParseMode::TypeName,
        "plpgsql-expr" => ParseMode::PlPgSqlExpr,
        "plpgsql-assign1" => ParseMode::PlPgSqlAssign1,
        "plpgsql-assign2" => ParseMode::PlPgSqlAssign2,
        "plpgsql-assign3" => ParseMode::PlPgSqlAssign3,
        _ => panic!("invalid fixture mode {name}"),
    }
}

fn policy(name: &str) -> BackslashQuote {
    match name {
        "off" => BackslashQuote::Off,
        "on" => BackslashQuote::On,
        "safe_encoding" => BackslashQuote::SafeEncoding,
        _ => panic!("invalid fixture policy {name}"),
    }
}

// [spec:pgq:req:grammar.diagnostics/test]
#[test]
fn parser_and_scanner_match_reference_diagnostics() {
    for case in cases() {
        let sql = case["sql"].as_str().unwrap();
        let mode = mode(case["mode"].as_str().unwrap());
        let options = Options::with_backslash_quote(policy(case["quote"].as_str().unwrap()));
        let scanned = Scanner::with_options(sql, options).find_map(Result::err);
        assert_eq!(diagnostic(scanned), case["scan_error"], "scan {case}");
        let parsed = run(sql, mode, options);
        assert_eq!(
            diagnostic(parsed.err()),
            case["parse_error"],
            "parse {case}"
        );
    }
}

#[test]
#[ignore = "set PG19_REFERENCE_DIR to an import with the mode-aware probes"]
fn verify_snapshots_against_both_pg19_builds() {
    let dir = std::env::var("PG19_REFERENCE_DIR").unwrap();
    for build in ["stock", "patched"] {
        let probe = Path::new(&dir).join(build).join("probe");
        for case in cases() {
            let out = Command::new(&probe)
                .args([
                    case["sql"].as_str().unwrap(),
                    "--mode",
                    case["mode"].as_str().unwrap(),
                    "--backslash-quote",
                    case["quote"].as_str().unwrap(),
                ])
                .output()
                .unwrap();
            assert!(out.status.success(), "{build}: {case}");
            let result: Json = serde_json::from_slice(&out.stdout).unwrap();
            for key in ["parse_error", "scan_error"] {
                assert_eq!(result[key], case[key], "{build} {key}: {case}");
            }
        }
    }
}

#[test]
fn absent_cursor_differs_from_first_character() {
    let get = |sql| {
        run(sql, ParseMode::Sql, Options::default())
            .unwrap_err()
            .cursor_position()
            .map(|p| p.get())
    };
    assert_eq!(get("SELECT E'\\xFF'"), None);
    assert_eq!(get(")"), Some(1));
    assert_eq!(get("SELECT '🙂', )"), Some(13));
}

// [spec:pgq:req:grammar.string-settings/test]
#[test]
fn escape_string_policy_is_per_call() {
    let sync = Barrier::new(3);
    thread::scope(|scope| {
        for setting in [
            BackslashQuote::Off,
            BackslashQuote::On,
            BackslashQuote::SafeEncoding,
        ] {
            let barrier = &sync;
            scope.spawn(move || {
                let options = Options::with_backslash_quote(setting);
                barrier.wait();
                for _ in 0..12 {
                    let sql = r"NOTIFY c, E'can\'t'";
                    let literal = run(sql, ParseMode::Sql, options);
                    if setting == BackslashQuote::Off {
                        let error = literal.unwrap_err();
                        assert_eq!((error.sqlstate, error.position), ("22P06", 11));
                    } else {
                        assert_eq!(literal.unwrap(), ["can't"]);
                    }
                    assert_eq!(
                        run(sql, ParseMode::Sql, Options::default()).unwrap(),
                        ["can't"]
                    );
                    let strings =
                        run(r"SELECT 'a\nb', E'a\nb', 'can''t'", ParseMode::Sql, options).unwrap();
                    assert_eq!(strings, [r"a\nb", "a\nb", "can't"]);
                }
            });
        }
    });
}
