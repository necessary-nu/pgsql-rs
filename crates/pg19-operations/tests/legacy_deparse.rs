use pg19_codec::protobuf::statements_protobuf;
use pg19_operations::deparse::{comments, protobuf_with_options, statements_with_options, Options};
use std::path::Path;

fn rendered(sql: &str, pretty: bool) -> Result<String, Box<dyn std::error::Error>> {
    let tree = pg19_parser::parse_statements(sql, Default::default())?;
    let options = Options {
        pretty_print: pretty,
        trailing_newline: true,
        ..Default::default()
    };
    let comments = comments(sql)?;
    let text = statements_with_options(&tree, options, &comments)?;
    let wire = statements_protobuf(&tree)?;
    assert_eq!(
        text,
        protobuf_with_options(&wire, options, &comments)?,
        "{sql}"
    );
    Ok(text)
}

#[test]
fn compact_c_goldens() {
    let corpus = include_str!("../../../test/deparse_tests.c");
    let mut failures = Vec::new();
    let mut count = 0;
    for (i, row) in corpus.lines().enumerate() {
        let row = row.trim();
        if row.starts_with('"') {
            let expected: String = serde_json::from_str(row.trim_end_matches(',')).unwrap();
            let actual = rendered(&expected, false).unwrap_or_else(|error| error.to_string());
            if actual != expected {
                failures.push((i + 1, expected, actual));
            }
            count += 1;
        }
    }
    assert!(count >= 448);
    assert!(
        failures.is_empty(),
        "{} differences: {failures:#?}",
        failures.len()
    );
}

#[test]
fn pretty_c_goldens() {
    let suite = include_str!("../../../test/deparse.c");
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/sql");
    let mut failures = Vec::new();
    for (array, subdir) in [
        ("deparseFilenames", "deparse"),
        ("deparseDepeszFilenames", "deparse-depesz"),
    ] {
        let start = format!("const char* {array}[] =");
        let rows = suite
            .split_once(&start)
            .unwrap()
            .1
            .split_once("};")
            .unwrap()
            .0;
        for row in rows.lines().map(str::trim).filter(|s| s.starts_with('"')) {
            let file = serde_json::Deserializer::from_str(row)
                .into_iter::<String>()
                .next()
                .unwrap()
                .unwrap();
            let sql = std::fs::read_to_string(base.join(subdir).join(&file)).unwrap();
            for span in pg19_operations::split_with_scanner(&sql).unwrap() {
                let expected = &sql[span.clone()];
                let actual = rendered(expected, true).unwrap_or_else(|error| error.to_string());
                if actual != expected {
                    failures.push((file.clone(), span.start, expected.to_owned(), actual));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} differences: {failures:#?}",
        failures.len()
    );
}
