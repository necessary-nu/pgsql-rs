use pgsql_operations::split_with_scanner;
use serde_json::Value;

// The ranges and diagnostics were captured from src/pg_query_split.c linked
// to the patched 19.0 backend. The first eight inputs also occur in libpg_query’s split_tests.c.
// [spec:pgq:req:split.scanner/test]
#[test]
fn matches_c_scanner_splitter() {
    for line in include_str!("scanner-cases.jsonl").lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        let sql = case["sql"].as_str().unwrap();
        let result = split_with_scanner(sql);
        if let Some(expected) = case["error"].as_str() {
            let error = result.expect_err(sql);
            assert_eq!(error.message, expected, "{sql:?}");
            assert_eq!(error.position as u64, case["position"], "{sql:?}");
            assert_eq!(error.sqlstate, "42601", "{sql:?}");
        } else {
            let spans: Vec<_> = result
                .expect(sql)
                .into_iter()
                .map(|range| {
                    assert!(sql.get(range.clone()).is_some(), "{sql:?} {range:?}");
                    [range.start, range.end]
                })
                .collect();
            assert_eq!(serde_json::json!(spans), case["ranges"], "{sql:?}");
        }
    }
}

#[test]
fn stops_at_first_nul() {
    let sql = "SELECT 'é'  \0; SELECT 'unterminated";
    let spans = split_with_scanner(sql).unwrap();
    assert_eq!(&sql[spans[0].clone()], "SELECT 'é'  ");
    assert_eq!(spans.len(), 1);
}
