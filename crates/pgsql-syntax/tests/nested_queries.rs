use pgsql_syntax::{parse, summary};

#[test]
fn nested_select_relations() {
    let cases: &[(&str, &[&str])] = &[
        ("select u.email, (select count(*) from enrollments e where e.user_id = u.id) as num_enrollments from users u", &["enrollments", "users"]),
        ("select u.* from (select * from users) u", &["users"]),
        ("select users.id from users where 1 = (select count(*) from user_roles)", &["user_roles", "users"]),
        ("
        select users.*
        from users
        where users.id IN (select user_roles.user_id from user_roles)
            and (users.created_at between '2016-06-01' and '2016-06-30')
    ", &["user_roles", "users"]),
        ("
        select users.*
        from users
        order by (
            select max(user_roles.role_id)
            from user_roles
            where user_roles.user_id = users.id
        )
    ", &["user_roles", "users"]),
        ("
        select users.*
        from users
        order by (
            select max(user_roles.role_id)
            from user_roles
            where user_roles.user_id = users.id
        ) asc, (
            select max(user_logins.role_id)
            from user_logins
            where user_logins.user_id = users.id
        ) desc
    ", &["user_logins", "user_roles", "users"]),
        ("
        select users.*
        from users
        group by (
            select max(user_roles.role_id)
            from user_roles
            where user_roles.user_id = users.id
        )
    ", &["user_roles", "users"]),
        ("
        select users.*
        from users
        group by (
            select max(user_roles.role_id)
            from user_roles
            where user_roles.user_id = users.id
        ), (
            select max(user_logins.role_id)
            from user_logins
            where user_logins.user_id = users.id
        )
    ", &["user_logins", "user_roles", "users"]),
        ("
        select users.*
        from users
        group by users.id
        having 1 > (
            select count(user_roles.role_id)
            from user_roles
            where user_roles.user_id = users.id
        )
    ", &["user_roles", "users"]),
        ("
        select users.*
        from users
        group by users.id
        having true and 1 > (
            select count(user_roles.role_id)
            from user_roles
            where user_roles.user_id = users.id
        )
    ", &["user_roles", "users"]),
        ("
        select foo.*
        from foo
        join ( select * from bar ) b
        on b.baz = foo.quux
    ", &["bar", "foo"]),
        ("
        SELECT *
        FROM foo
        INNER JOIN join_a ON foo.id = join_a.id AND join_a.id IN (
            SELECT id
            FROM sub_a
            INNER JOIN sub_b ON sub_a.id = sub_b.id AND sub_b.id IN (
                SELECT id
                FROM sub_c
                INNER JOIN sub_d ON sub_c.id IN (SELECT id from sub_e)
            )
        )
        INNER JOIN join_b ON foo.id = join_b.id AND join_b.id IN (
          SELECT id FROM sub_f
        )
    ", &["foo", "join_a", "join_b", "sub_a", "sub_b", "sub_c", "sub_d", "sub_e", "sub_f"]),
        ("
        SELECT GREATEST(
            date_trunc($1, $2::timestamptz) + $3::interval,
            COALESCE(
                (
                    SELECT first_aggregate_starts_at
                    FROM schema_aggregate_infos
                    WHERE base_table = $4 LIMIT $5
                ),
                now() + $6::interval
            )
        ) AS first_hourly_start_ts
    ", &["schema_aggregate_infos"]),
        ("
        SELECT
        CASE
            WHEN id IN (SELECT foo_id FROM when_a) THEN (SELECT MAX(id) FROM then_a)
            WHEN id IN (SELECT foo_id FROM when_b) THEN (SELECT MAX(id) FROM then_b)
            ELSE (SELECT MAX(id) FROM elsey)
        END
        FROM foo
    ", &["elsey", "foo", "then_a", "then_b", "when_a", "when_b"]),
    ];
    for &(sql, expected) in cases {
        let tree = parse(sql, 0).unwrap();
        let report = summary(sql, 0, -1).unwrap();
        assert!(
            tree.warnings.is_empty() && report.warnings.is_empty(),
            "{sql}"
        );
        assert_eq!(tree.statement_types(), ["SelectStmt"], "{sql}");
        assert_eq!(report.statement_types(), ["SelectStmt"], "{sql}");
        for mut names in [
            tree.tables(),
            tree.select_tables(),
            report.tables(),
            report.select_tables(),
        ] {
            names.sort();
            assert_eq!(names, expected, "{sql}");
        }
    }
}

#[test]
fn set_operands_share_cte_bindings() {
    for operator in ["UNION", "EXCEPT", "INTERSECT"] {
        let sql = format!(
            "WITH cte_a AS (SELECT * FROM table_a), cte_b AS (SELECT * FROM table_b)
            SELECT id FROM table_c LEFT JOIN cte_b ON table_c.id = cte_b.c_id
            {operator} SELECT * FROM cte_a"
        );
        let parsed = parse(&sql, 0).unwrap();
        let report = summary(&sql, 0, -1).unwrap();
        assert!(
            parsed.warnings.is_empty() && report.warnings.is_empty(),
            "{operator}"
        );
        assert_eq!(parsed.statement_types(), ["SelectStmt"], "{operator}");
        assert_eq!(report.statement_types(), ["SelectStmt"], "{operator}");
        for mut tables in [parsed.tables(), report.tables()] {
            tables.sort();
            assert_eq!(tables, ["table_a", "table_b", "table_c"], "{operator}");
        }
        for mut names in [parsed.cte_names, report.cte_names] {
            names.sort();
            assert_eq!(names, ["cte_a", "cte_b"], "{operator}");
        }
    }
}
