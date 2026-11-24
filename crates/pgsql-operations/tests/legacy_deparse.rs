use pgsql_codec::protobuf::statements_protobuf;
use pgsql_operations::deparse::{
    comments, protobuf_with_options, statements_with_options, Options,
};
use std::path::{Path, PathBuf};

fn rendered(sql: &str, pretty: bool) -> Result<String, Box<dyn std::error::Error>> {
    let tree = pgsql_grammar::parse_statements(sql, Default::default())?;
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
    let corpus = include_str!("../../../tools/pg19/libpg_query/test/deparse_tests.c");
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

fn pretty_fixtures(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            files.extend(pretty_fixtures(&path)?);
        } else if matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("sql" | "psql")
        ) {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

#[test]
fn pretty_c_goldens() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/pg19/libpg_query/test/sql");
    let mut failures = Vec::new();
    for (suite, count) in [("deparse", 15), ("deparse-depesz", 141)] {
        let files = pretty_fixtures(&base.join(suite)).unwrap();
        assert_eq!(files.len(), count, "fixture count for {suite}");
        for file in files {
            let sql = std::fs::read_to_string(&file).unwrap();
            for span in pgsql_operations::split_with_scanner(&sql).unwrap() {
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
