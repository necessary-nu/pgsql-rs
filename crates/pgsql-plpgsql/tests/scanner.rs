use pgsql_plpgsql::{token::*, BodyScanner, Lookup, Namespace, Options, ParseMode, Resolved};
use serde_json::{json, Value};
use std::process::Command;

struct Variables;

impl Namespace for Variables {
    fn lookup(&mut self, names: &[String]) -> Option<Resolved> {
        let (datum, names_used) = match names.first()?.as_str() {
            "perform" => (4, 1),
            "keyword" => (5, 1),
            "new" if names.len() > 1 => (9, 2),
            _ => return None,
        };
        Some(Resolved { datum, names_used })
    }
}

#[test]
fn contextual_lookup_preserves_compound_spans() {
    let body = "BEGIN PERFORM perform; perform[1] := NEW . name.extra; END";
    let mut scanner = BodyScanner::new(body, Options::default());
    let mut ns = Variables;
    let mut take = |scanner: &mut BodyScanner| scanner.next_token(&mut ns).unwrap();
    assert_eq!(take(&mut scanner).id, K_BEGIN);
    assert_eq!(take(&mut scanner).id, K_PERFORM);
    assert_eq!(take(&mut scanner).datum, Some(4));
    assert_eq!(take(&mut scanner).id, 59);
    let assign = take(&mut scanner);
    assert_eq!(assign.datum, Some(4));
    scanner.push_back(assign);
    let f = scanner
        .read_sql(&mut Variables, &[59], ";", ParseMode::PlPgSqlAssign1, false)
        .unwrap();
    assert_eq!(f.query, "perform[1] := NEW . name.extra");
    assert_eq!(f.terminator.id, 59);
    assert_eq!(take(&mut scanner).id, K_END);
    let mut scanner = BodyScanner::new(
        "NEW . name.extra keyword NEW.name keyword",
        Options::default(),
    );
    scanner.lookup = Lookup::Expression;
    let field = take(&mut scanner);
    assert_eq!(field.names, ["new", "name"]);
    assert_eq!(field.datum, Some(9));
    assert_eq!(field.span, 0..16);
    assert_eq!(take(&mut scanner).id, T_WORD);
    scanner.lookup = Lookup::Declare;
    assert_eq!(take(&mut scanner).id, T_CWORD);
    scanner.lookup = Lookup::Normal;
    assert_eq!(take(&mut scanner).datum, Some(5));
}

fn tokens(source: &str) -> Value {
    let mut scanner = BodyScanner::new(source, Options::default());
    let mut out = Vec::new();
    loop {
        match scanner.next_token(&mut ()) {
            Ok(t) if t.id == 0 => return Value::Array(out),
            Ok(t) => out.push(json!([t.id, t.span.start, t.span.end])),
            Err(e) => return json!({"message":e.message,"position":e.position}),
        }
    }
}

#[test]
fn core_and_pl_tokens_match_bison_symbols() {
    use pgsql_tables::plpgsql::{symbol_name, terminal, PRODUCTION_COUNT, STATE_COUNT};
    assert_eq!((STATE_COUNT, PRODUCTION_COUNT), (336, 255));
    assert_eq!(symbol_name(terminal(K_BEGIN)), Some("K_BEGIN"));
    assert_eq!(
        tokens("# << abc >> N'x' .foo -- ok"),
        json!([
            [35, 0, 1],
            [280, 2, 4],
            [277, 5, 8],
            [281, 9, 11],
            [277, 12, 13],
            [261, 13, 16],
            [46, 17, 18],
            [277, 18, 21]
        ])
    );
}

#[test]
#[ignore = "requires the configured PG19 PL/pgSQL probe"]
fn compare_upstream_scanner() {
    let run = std::path::PathBuf::from(std::env::var_os("PG19_REFERENCE_DIR").unwrap());
    let mut bodies: Vec<String> = vec![
        "A.b.c.d .x NEW . \"p\" $$';$$ <<< >> ",
        "A.* A.end A.$1 A. $1",
        "a.a.a.a a.a..f",
        "# <<é>>",
        "E'\\101' X'ac' U&'x' U&\"a\" $tag$BEGIN;END$tag$ '\n'\n'a'",
        "N'---' nchar $01023 $1.x $2.$1 03.42 +4 $0",
        "9_0.2 /* END */ BEGIN PERFORM y; END; -- ignored",
        "nested /* foo /* ; */ ; */ after",
        "/* unterminated",
        "$$unterminated",
        "[]()<<>> # := = => <> SELECT current_schema except exists",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for &(word, _) in RESERVED.iter().chain(UNRESERVED) {
        bodies.push(format!("{word} \"{word}\" {word}.{word} \"{word}\".{word}"));
    }
    let output = Command::new(run.join("plpgsql-probe"))
        .arg("--scan")
        .args(&bodies)
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.lines().count(), bodies.len());
    for (line, source) in stdout.lines().zip(bodies) {
        let expected: Value = serde_json::from_str(line).unwrap();
        assert_eq!(tokens(&source), expected, "{source:?}");
    }
}
