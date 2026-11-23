use pgsql_syntax::{parse, summary};

type Column<'a> = (Option<&'a str>, &'a str);

fn check(sql: &str, expected: &[Column<'_>]) {
    let parsed = parse(sql, 0).unwrap();
    let report = summary(sql, 0, -1).unwrap();
    let mut parsed_cols: Vec<_> = parsed
        .filter_columns
        .iter()
        .map(|(t, c)| (t.as_deref(), c.as_str()))
        .collect();
    let mut summary_cols: Vec<_> = report
        .filter_columns
        .iter()
        .map(|c| {
            assert!(c.schema_name.is_none(), "{sql}");
            (c.table_name.as_deref(), c.column.as_str())
        })
        .collect();
    parsed_cols.sort();
    summary_cols.sort();
    assert_eq!(parsed_cols, expected, "{sql}");
    assert_eq!(summary_cols, expected, "{sql}");
}

#[test]
fn filter_columns_follow_where_expressions() {
    let cases: &[(&str, &[Column<'_>])] = &[
        (
            "SELECT * FROM x WHERE y = $1 AND z = 1",
            &[(None, "y"), (None, "z")],
        ),
        (
            "SELECT * FROM x WHERE x.y = $1 AND x.z = 1",
            &[(Some("x"), "y"), (Some("x"), "z")],
        ),
        (
            "WITH a AS (SELECT * FROM x WHERE x.y = $1 AND x.z = 1) SELECT * FROM a WHERE b = 5",
            &[(None, "b"), (Some("x"), "y"), (Some("x"), "z")],
        ),
        (
            "SELECT * FROM x WHERE x.y IS TRUE AND x.z IS NOT FALSE",
            &[(Some("x"), "y"), (Some("x"), "z")],
        ),
        (
            "SELECT * FROM x WHERE x.y IS NULL AND x.z IS NOT NULL",
            &[(Some("x"), "y"), (Some("x"), "z")],
        ),
        (
            "SELECT * FROM x WHERE x.y = COALESCE(z.a, z.b)",
            &[(Some("x"), "y"), (Some("z"), "a"), (Some("z"), "b")],
        ),
        (
            "SELECT a, y, z FROM x WHERE x.y = $1 AND x.z = 1",
            &[(Some("x"), "y"), (Some("x"), "z")],
        ),
        (
            "SELECT * FROM x WHERE x.y = $1 AND x.z = 1 ORDER BY a, b",
            &[(Some("x"), "y"), (Some("x"), "z")],
        ),
    ];
    for &(sql, expected) in cases {
        check(sql, expected);
    }
}

#[test]
fn filters_from_both_set_operands() {
    for op in [
        "UNION",
        "UNION ALL",
        "EXCEPT",
        "EXCEPT ALL",
        "INTERSECT",
        "INTERSECT ALL",
    ] {
        let sql = format!("SELECT * FROM x where y = $1 {op} SELECT * FROM x where z = $2");
        check(&sql, &[(None, "y"), (None, "z")]);
    }
}
