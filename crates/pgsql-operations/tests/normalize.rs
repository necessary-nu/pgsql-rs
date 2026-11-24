use pgsql_operations::{normalize, normalize_utility};
use serde_json::Value;

// [spec:pgq:req:normalize.substitutions/test]
// [spec:pgq:req:normalize.literal-spans/test]
// [spec:pgq:req:normalize.grouping/test]
// [spec:pgq:req:normalize.utility/test]
#[test]
fn pg19_normalization_matches_in_both_modes() {
    for row in include_str!("normalize.jsonl")
        .lines()
        .chain(include_str!("normalize-legacy.jsonl").lines())
    {
        let case: Value = serde_json::from_str(row).unwrap();
        let sql = case["sql"].as_str().unwrap();
        assert_eq!(normalize(sql).unwrap(), case["full"], "{sql}");
        assert_eq!(normalize_utility(sql).unwrap(), case["utility"], "{sql}");
    }
}

#[test]
#[ignore = "compares the offline vectors with a pinned libpg_query checkout"]
fn offline_normalization_vectors_match_upstream() {
    let run = std::env::var_os("PG19_REFERENCE_DIR").expect("PG19_REFERENCE_DIR");
    let upstream = std::path::PathBuf::from(run).join("libpg_query/test");
    let cases: Vec<Value> = include_str!("normalize-legacy.jsonl")
        .lines()
        .map(|row| serde_json::from_str(row).unwrap())
        .collect();
    for (file, mode) in [
        ("normalize_tests.c", "full"),
        ("normalize_utility_tests.c", "utility"),
    ] {
        let source = std::fs::read_to_string(upstream.join(file)).unwrap();
        let strings: Vec<String> = source
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with('"'))
            .map(|line| serde_json::from_str(line.trim_end_matches(',')).unwrap())
            .collect();
        let (pairs, trailing) = strings.as_chunks::<2>();
        assert!(trailing.is_empty(), "{file}");
        assert_eq!(pairs.len(), cases.len(), "{file}");
        for ([sql, expected], case) in pairs.iter().zip(&cases) {
            assert_eq!(sql, &case["sql"], "{file}");
            assert_eq!(expected, &case[mode], "{file}");
        }
    }
}

#[test]
fn diagnostics_and_nul_termination() {
    assert_eq!(normalize("SELECT 3\0$9000").unwrap(), "SELECT $1");
    let bad = "SELECT 3, )";
    let expected = pgsql_grammar::parse_statements(bad, Default::default()).unwrap_err();
    assert_eq!(normalize(bad).unwrap_err(), expected);
    assert_eq!(normalize_utility(bad).unwrap_err(), expected);
}

#[test]
fn normalize_deep_union_on_a_small_stack() {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let sql = vec!["SELECT 7"; 6000].join(" UNION ALL ");
            let expected = (1..=6000)
                .map(|i| format!("SELECT ${i}"))
                .collect::<Vec<_>>()
                .join(" UNION ALL ");
            assert_eq!(normalize(&sql).unwrap(), expected);
        })
        .unwrap()
        .join()
        .unwrap();
}
