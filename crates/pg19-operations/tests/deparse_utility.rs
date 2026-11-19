use pg19_operations::deparse;
#[test]
fn utilities_retain_compact_sql_from_the_c_deparser() {
    let mut differences = Vec::new();
    for line in include_str!("deparse-utility.jsonl")
        .lines()
        .chain(include_str!("deparse-objects.jsonl").lines())
        .chain(include_str!("deparse-table.jsonl").lines())
        .chain(include_str!("deparse-alter.jsonl").lines())
        .chain(include_str!("deparse-privileges-index.jsonl").lines())
    {
        let [sql, expected]: [String; 2] = serde_json::from_str(line).unwrap();
        let trees = pg19_parser::parse_statements(&sql, Default::default()).unwrap();
        let result = deparse::statements(&trees);
        if result.as_ref().ok() != Some(&expected) {
            differences.push((sql, expected, result));
        }
    }
    assert!(differences.is_empty(), "{differences:#?}");
}
