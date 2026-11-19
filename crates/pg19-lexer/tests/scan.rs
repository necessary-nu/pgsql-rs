use std::{error::Error, fs, path::Path, process::Command};

use pg19_lexer::{KeywordKind, Kind, Options, Scanner, Value};
use serde_json::{json, Value as Json};

fn check_tokens(sql: &str, expected: &Json, options: Options) {
    let mut observed = Vec::new();
    let mut err = Json::Null;
    for token in Scanner::with_options(sql, options) {
        match token {
            Ok(t) => observed.push(
                json!({"id":t.kind.id(), "name":t.kind.name(), "start":t.start, "end":t.end}),
            ),
            Err(e) => {
                err = json!({"message": e.message, "sqlstate": e.sqlstate, "position": e.position})
            }
        }
    }
    let wanted = expected["tokens"].as_array().unwrap();
    for (a, b) in observed.iter().zip(wanted) {
        if a != b {
            let start = b["start"].as_u64().unwrap() as usize;
            panic!("first token difference at {start}, {:?} vs {:?}", a, b);
        }
    }
    assert_eq!(
        observed.len(),
        wanted.len(),
        "different token counts, scan error {err}"
    );
    assert_eq!(err, expected["scan_error"]);
}

fn grammar(sql: &str) -> Vec<pg19_lexer::Token> {
    Scanner::new(sql)
        .grammar()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

// [spec:pgq:req:lex.byte-spans/test]
// [spec:pgq:req:lex.comments/test]
#[test]
fn agrees_with_reference_snapshots() -> Result<(), Box<dyn Error>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/pg19/fixtures");
    for row in fs::read_to_string(dir.join("cases.txt"))?.lines() {
        let mut fields = row.split_ascii_whitespace();
        let name = fields.next().ok_or("empty fixture name")?;
        let args: Vec<_> = fields.collect();
        let options = Options {
            disable_backslash_quote: args
                .chunks_exact(2)
                .any(|pair| pair == ["--backslash-quote", "off"]),
        };
        let sql = fs::read_to_string(dir.join(format!("{name}.sql")))?;
        let expected =
            serde_json::from_slice(&fs::read(dir.join(format!("{name}.patched.json")))?)?;
        check_tokens(&sql, &expected, options);
    }
    Ok(())
}

// [spec:pgq:req:lex.keywords/test]
#[test]
fn keyword_categories_and_identifier_spelling() {
    let tokens = grammar("selEct integer join abort \"SELECT\" MIXÉ");
    assert_eq!(
        tokens.iter().map(|t| t.keyword).collect::<Vec<_>>(),
        [
            Some(KeywordKind::Reserved),
            Some(KeywordKind::ColumnName),
            Some(KeywordKind::TypeFunctionName),
            Some(KeywordKind::Unreserved),
            None,
            None
        ]
    );
    assert_eq!(tokens[4].value, Value::Text("SELECT".into()));
    assert_eq!(tokens[5].value, Value::Text("mixÉ".into()));
}

// [spec:pgq:req:lex.numbers-and-parameters/test]
#[test]
fn numeric_classes_and_parameter_bounds() {
    let t = grammar("0x_ff 0b1_0 2_147_483_648 $001alias 1..2 .25E-2");
    assert_eq!(t[0].value, Value::Integer(255));
    assert_eq!(t[1].value, Value::Integer(2));
    assert_eq!(t[2].kind, Kind::FCONST);
    assert_eq!(t[3].value, Value::Integer(1));
    assert_eq!(t[4].kind, Kind::IDENT);
    assert_eq!(t[6].kind, Kind::DOT_DOT);
    assert_eq!(t[8].value, Value::Text(".25E-2".into()));
    assert_eq!(
        Scanner::new("$2147483648")
            .next()
            .unwrap()
            .unwrap_err()
            .position,
        1
    );
}

// [spec:pgq:req:lex.quoted-values/test]
#[test]
fn literals_are_decoded_in_their_own_states() {
    let t = grammar(
        r#"B'12' X'fg' $tag$$other$tag$ E'\xC3\251\uD83D\uDE42' 'ab''c'
    -- continue
    'd'"#,
    );
    let text: Vec<_> = t.into_iter().map(|t| t.value).collect();
    assert_eq!(
        text,
        ["b12", "xfg", "$other", "é🙂", "ab'cd"].map(|s| Value::Text(s.into()))
    );
}

// [spec:pgq:req:lex.parser-lookahead/test]
#[test]
fn lookahead_preserves_positions_and_skips_comments() {
    let sql = "NOT /* one */ IN NULLS LAST WITH TIME FORMAT JSON WITHOUT TIME NOT TRUE";
    let t = grammar(sql);
    assert_eq!(
        t.iter().map(|t| t.kind.name()).collect::<Vec<_>>(),
        [
            "NOT_LA",
            "IN_P",
            "NULLS_LA",
            "LAST_P",
            "WITH_LA",
            "TIME",
            "FORMAT_LA",
            "JSON",
            "WITHOUT_LA",
            "TIME",
            "NOT",
            "TRUE_P"
        ]
    );
    for t in t {
        assert_eq!(sql[t.start..t.end].split_whitespace().count(), 1);
    }
    assert_eq!(Scanner::new(sql).next().unwrap().unwrap().kind, Kind::NOT);
}

// [spec:pgq:req:lex.unicode-escape/test]
#[test]
fn unicode_escape_clause_consumes_three_tokens() {
    let t = grammar(r#"U&"d!0061t" UESCAPE '!' U&'\+01F642' U&'\D83D\DE42'"#);
    assert_eq!(t.len(), 3);
    assert_eq!(t[0].value, Value::Text("dat".into()));
    assert_eq!(t[1].value, Value::Text("🙂".into()));
    assert_eq!(t[2].value, t[1].value);
    let err = Scanner::new(r#"U&'\D800x'"#)
        .grammar()
        .next()
        .unwrap()
        .unwrap_err();
    assert_eq!(
        (err.position, err.message.as_str()),
        (9, "invalid Unicode surrogate pair")
    );
}

#[test]
#[ignore = "set PG19_REFERENCE_DIR to a completed PG19 import"]
fn matches_regression_sql_token_streams() -> Result<(), Box<dyn Error>> {
    let root = std::env::var("PG19_REFERENCE_DIR")?;
    let root = Path::new(&root);
    let mut checked = 0;
    for entry in fs::read_dir(root.join("patched-source/src/test/regress/sql"))? {
        let path = entry?.path();
        if path.extension().and_then(|s| s.to_str()) != Some("sql") {
            continue;
        }
        let bytes = fs::read(&path)?;
        let Ok(sql) = std::str::from_utf8(&bytes) else {
            continue;
        };
        // Keep OS argument lengths bounded; each chunk is an independent input to both scanners.
        let mut start = 0;
        while start < sql.len() {
            let mut end = (start + 96 * 1024).min(sql.len());
            while !sql.is_char_boundary(end) {
                end -= 1;
            }
            let chunk = &sql[start..end];
            let out = Command::new(root.join("patched/probe"))
                .arg(chunk)
                .output()?;
            assert!(
                out.status.success(),
                "{} byte {start}: {:?}",
                path.display(),
                out.status
            );
            let expected = serde_json::from_slice(&out.stdout)?;
            eprintln!("{} byte {start}", path.display());
            check_tokens(chunk, &expected, Options::default());
            checked += 1;
            start = end;
        }
    }
    assert!(checked > 200);
    Ok(())
}
