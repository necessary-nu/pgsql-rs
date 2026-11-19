use pg19_operations::{normalize, normalize_utility};
use serde_json::Value;

// [spec:pgq:req:normalize.substitutions/test]
// [spec:pgq:req:normalize.literal-spans/test]
// [spec:pgq:req:normalize.grouping/test]
// [spec:pgq:req:normalize.utility/test]
#[test]
fn pg19_normalization_matches_in_both_modes() {
    for row in include_str!("normalize.jsonl").lines() {
        let case: Value = serde_json::from_str(row).unwrap();
        let sql = case["sql"].as_str().unwrap();
        assert_eq!(normalize(sql).unwrap(), case["full"], "{sql}");
        assert_eq!(normalize_utility(sql).unwrap(), case["utility"], "{sql}");
    }
}

#[test]
fn original_c_normalization_vectors() {
    let mut failures = Vec::new();
    for (cases, utility) in [
        (include_str!("../../../test/normalize_tests.c"), false),
        (
            include_str!("../../../test/normalize_utility_tests.c"),
            true,
        ),
    ] {
        let strings: Vec<String> = cases
            .lines()
            .map(str::trim)
            .filter(|s| s.starts_with('"'))
            .map(|s| serde_json::from_str(s.trim_end_matches(',')).unwrap())
            .collect();
        let (pairs, trailing) = strings.as_chunks::<2>();
        assert!(
            trailing.is_empty(),
            "normalization fixture needs an expected result"
        );
        for [input, expected] in pairs {
            let got = if utility {
                normalize_utility(input)
            } else {
                normalize(input)
            };
            if got.as_ref().ok() != Some(expected) {
                failures.push(format!("{input}: {got:?} vs {expected:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn diagnostics_and_nul_termination() {
    assert_eq!(normalize("SELECT 3\0$9000").unwrap(), "SELECT $1");
    let bad = "SELECT 3, )";
    let expected = pg19_parser::parse_statements(bad, Default::default()).unwrap_err();
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
