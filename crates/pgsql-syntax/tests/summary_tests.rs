#![allow(non_snake_case)]
#![cfg(test)]

#[cfg(test)]
use itertools::sorted;

use pgsql_syntax::{summary, Error, FilterColumn};

#[macro_use]
mod support;

#[path = "summary/references.rs"]
mod references;

#[test]
fn it_parses_simple_query() {
    let result = summary("SELECT * FROM test WHERE a = 1", 0, -1).unwrap();

    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 1);
    assert_eq!(result.aliases.is_empty(), true);
    assert_eq!(result.cte_names.len(), 0);
    assert_eq!(result.functions.len(), 0);
    assert_eq!(result.filter_columns.len(), 1);
    assert!(result.truncated_query.is_empty());
    assert_eq!(result.statement_types(), ["SelectStmt"]);
}

#[test]
fn it_parses_simple_query_with_alias() {
    let result = summary("SELECT * FROM test AS x WHERE a = 1", 0, -1).unwrap();

    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 1);
    assert_eq!(result.aliases.len(), 1);
    assert_eq!(result.aliases["x"], "test");
    assert_eq!(result.cte_names.len(), 0);
    assert_eq!(result.functions.len(), 0);
    assert_eq!(result.filter_columns.len(), 1);
    assert!(result.truncated_query.is_empty());
    assert_eq!(result.statement_types(), ["SelectStmt"]);
}

#[test]
fn it_parses_query_with_nested_select_where() {
    let result = summary(
        "SELECT * FROM test WHERE col1 = (SELECT col2 FROM test2 WHERE col3 = 123)",
        0,
        -1,
    )
    .unwrap();

    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 2);
    assert_eq!(result.aliases.len(), 0);
    assert_eq!(result.cte_names.len(), 0);
    assert_eq!(result.functions.len(), 0);
    let filter_columns: Vec<&FilterColumn> = sorted(&result.filter_columns).collect();
    assert_eq!(
        filter_columns,
        [
            &FilterColumn {
                schema_name: None,
                table_name: None,
                column: "col1".to_string()
            },
            &FilterColumn {
                schema_name: None,
                table_name: None,
                column: "col3".to_string()
            },
        ]
    );
    assert_eq!(result.statement_types(), ["SelectStmt"]);
}

#[test]
fn it_handles_errors() {
    let error = summary("CREATE RANDOM ix_test ON contacts.person;", 0, -1)
        .err()
        .unwrap();
    assert_eq!(
        error,
        Error::Parse("syntax error at or near \"RANDOM\"".into())
    );

    let error = summary("SELECT 'ERR", 0, -1).err().unwrap();
    assert_eq!(
        error,
        Error::Parse("unterminated quoted string at or near \"'ERR\"".into())
    );
}

#[test]
fn it_serializes_as_json() {
    let result = summary("SELECT 1 FROM pg_class", 0, -1).unwrap();
    let json = serde_json::to_string(&result.protobuf);

    assert!(json.is_ok(), "Protobuf should be serializable: {json:?}");
}

#[test]
fn it_handles_basic_query() {
    let query = r#"SELECT * FROM "t0""#;
    let result = summary(query, 0, -1).unwrap();
    assert_eq!(result.tables().len(), 1);
    assert_eq!(result.tables(), ["t0"]);
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.aliases.len(), 0);
    assert_eq!(result.cte_names.len(), 0);
    assert_eq!(result.functions.len(), 0);
    assert_eq!(result.filter_columns.len(), 0);
    assert!(result.truncated_query.is_empty());
}

#[test]
fn it_handles_join_expression() {
    let query = r#"SELECT * FROM "t0" JOIN "t1" ON (1)"#;
    let result = summary(query, 0, -1).unwrap();
    assert_eq!(result.tables().len(), 2);
    assert_eq!(result.statement_types(), ["SelectStmt"]);
}

#[test]
fn it_handles_recursion_without_error() {
    // The Ruby version of pg_query fails here because of Ruby protobuf limitations
    let query = r#"SELECT * FROM "t0"
        JOIN "t1" ON (1) JOIN "t2" ON (1) JOIN "t3" ON (1) JOIN "t4" ON (1) JOIN "t5" ON (1)
        JOIN "t6" ON (1) JOIN "t7" ON (1) JOIN "t8" ON (1) JOIN "t9" ON (1) JOIN "t10" ON (1)
        JOIN "t11" ON (1) JOIN "t12" ON (1) JOIN "t13" ON (1) JOIN "t14" ON (1) JOIN "t15" ON (1)
        JOIN "t16" ON (1) JOIN "t17" ON (1) JOIN "t18" ON (1) JOIN "t19" ON (1) JOIN "t20" ON (1)
        JOIN "t21" ON (1) JOIN "t22" ON (1) JOIN "t23" ON (1) JOIN "t24" ON (1) JOIN "t25" ON (1)
        JOIN "t26" ON (1) JOIN "t27" ON (1) JOIN "t28" ON (1) JOIN "t29" ON (1)"#;
    let result = summary(query, 0, -1).unwrap();
    assert_eq!(result.tables().len(), 30);
    assert_eq!(result.statement_types(), ["SelectStmt"]);
}

#[test]
fn it_parses_real_queries() {
    let query = "
        SELECT memory_total_bytes, memory_free_bytes, memory_pagecache_bytes, memory_buffers_bytes, memory_applications_bytes,
            (memory_swap_total_bytes - memory_swap_free_bytes) AS swap, date_part($0, s.collected_at) AS collected_at
        FROM snapshots s JOIN system_snapshots ON (snapshot_id = s.id)
        WHERE s.database_id = $0 AND s.collected_at BETWEEN $0 AND $0
        ORDER BY collected_at";
    let result = summary(query, 0, -1).unwrap();
    let tables: Vec<String> = sorted(result.tables()).collect();
    let select_tables: Vec<String> = sorted(result.select_tables()).collect();
    assert_eq!(tables, ["snapshots", "system_snapshots"]);
    assert_eq!(select_tables, ["snapshots", "system_snapshots"]);
    assert_eq!(result.statement_types(), ["SelectStmt"]);
}

#[test]
fn it_parses_empty_queries() {
    let result = summary("-- nothing", 0, -1).unwrap();
    assert_eq!(result.statement_types().len(), 0);
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.aliases.is_empty(), true);
    assert_eq!(result.cte_names.len(), 0);
    assert_eq!(result.functions.len(), 0);
    assert_eq!(result.filter_columns.len(), 0);
    assert!(result.truncated_query.is_empty());
}

#[test]
fn it_parses_floats_with_leading_dot() {
    let result = summary("SELECT .1", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
}

#[test]
fn it_parses_bit_strings_hex_notation() {
    let result = summary("SELECT X'EFFF'", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
}

#[test]
fn it_parses_ALTER_TABLE() {
    let result = summary("ALTER TABLE test ADD PRIMARY KEY (gid)", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["test"]);
    assert_eq!(result.ddl_tables(), ["test"]);
    assert_eq!(result.statement_types(), ["AlterTableStmt"]);
}

#[test]
fn it_parses_SET() {
    let result = summary("SET statement_timeout=1", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.ddl_tables().len(), 0);
    assert_eq!(result.statement_types(), ["VariableSetStmt"]);
}

#[test]
fn it_parses_SHOW() {
    let result = summary("SHOW work_mem", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.statement_types(), ["VariableShowStmt"]);
}

#[test]
fn it_parses_COPY() {
    let result = summary("COPY test (id) TO stdout", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["test"]);
    assert_eq!(result.statement_types(), ["CopyStmt"]);
}

#[test]
fn it_parses_DROP_TABLE() {
    let result = summary("drop table abc.test123 cascade", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["abc.test123"]);
    assert_eq!(result.ddl_tables(), ["abc.test123"]);
    assert_eq!(result.statement_types(), ["DropStmt"]);

    let result = summary("drop table abc.test123, test", 0, -1).unwrap();
    let tables: Vec<String> = sorted(result.tables()).collect();
    let ddl_tables: Vec<String> = sorted(result.ddl_tables()).collect();
    assert_eq!(tables, ["abc.test123", "test"]);
    assert_eq!(ddl_tables, ["abc.test123", "test"]);
}

#[test]
fn it_parses_COMMIT() {
    let result = summary("COMMIT", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.statement_types(), ["TransactionStmt"]);
}

#[test]
fn it_parses_CHECKPOINT() {
    let result = summary("CHECKPOINT", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.statement_types(), ["CheckPointStmt"]);
}

#[test]
fn it_parses_VACUUM() {
    let result = summary("VACUUM my_table", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["my_table"]);
    assert_eq!(result.ddl_tables(), ["my_table"]);
    assert_eq!(result.statement_types(), ["VacuumStmt"]);
}

#[test]
fn it_parses_MERGE() {
    let result = summary(
        "WITH cte AS (SELECT * FROM g.other_table CROSS JOIN p) MERGE INTO my_table USING cte ON (id=oid) WHEN MATCHED THEN UPDATE SET a=b WHEN NOT MATCHED THEN INSERT (id, a) VALUES (oid, b);", 0,
        -1,
    )
    .unwrap();
    assert_eq!(result.warnings.len(), 0);

    let select_tables: Vec<String> = sorted(result.select_tables()).collect();
    assert_eq!(select_tables, ["g.other_table", "p"]);

    let dml_tables: Vec<String> = sorted(result.dml_tables()).collect();
    assert_eq!(dml_tables, ["my_table"]);

    let all_tables: Vec<String> = sorted(result.tables()).collect();
    assert_eq!(all_tables, ["g.other_table", "my_table", "p"]);

    assert_eq!(result.cte_names, ["cte"]);
    assert_eq!(result.statement_types(), ["MergeStmt", "SelectStmt"]);
}

#[test]
fn it_parses_EXPLAIN() {
    let result = summary("EXPLAIN DELETE FROM test", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["test"]);
    assert_eq!(result.statement_types(), ["ExplainStmt", "DeleteStmt"]);
}

#[test]
fn it_parses_SELECT_INTO() {
    let result = summary("CREATE TEMP TABLE test AS SELECT 1", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["test"]);
    assert_eq!(result.ddl_tables(), ["test"]);
    assert_eq!(
        result.statement_types(),
        ["CreateTableAsStmt", "SelectStmt"]
    );
}

#[test]
fn it_parses_LOCK() {
    let result = summary(
        "LOCK TABLE public.schema_migrations IN ACCESS SHARE MODE",
        0,
        -1,
    )
    .unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["public.schema_migrations"]);
    assert_eq!(result.statement_types(), ["LockStmt"]);
}

#[test]
fn it_parses_CREATE_TABLE() {
    let result = summary("CREATE TABLE test (a int4)", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["test"]);
    assert_eq!(result.ddl_tables(), ["test"]);
    assert_eq!(result.statement_types(), ["CreateStmt"]);
}

#[test]
fn it_parses_CREATE_TABLE_AS() {
    let result = summary("CREATE TABLE foo AS SELECT * FROM bar;", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    let tables: Vec<String> = sorted(result.tables()).collect();
    assert_eq!(tables, ["bar", "foo"]);
    assert_eq!(result.ddl_tables(), ["foo"]);
    assert_eq!(result.select_tables(), ["bar"]);
    assert_eq!(
        result.statement_types(),
        ["CreateTableAsStmt", "SelectStmt"]
    );

    let sql = "CREATE TABLE foo AS SELECT id FROM bar UNION SELECT id from baz;";
    let result = summary(sql, 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    let tables: Vec<String> = sorted(result.tables()).collect();
    let select_tables: Vec<String> = sorted(result.select_tables()).collect();
    assert_eq!(tables, ["bar", "baz", "foo"]);
    assert_eq!(result.ddl_tables(), ["foo"]);
    assert_eq!(select_tables, ["bar", "baz"]);
    assert_eq!(
        result.statement_types(),
        ["CreateTableAsStmt", "SelectStmt"]
    );
}

#[test]
fn it_fails_to_parse_CREATE_TABLE_WITH_OIDS() {
    let error = summary("CREATE TABLE test (a int4) WITH OIDS", 0, -1)
        .err()
        .unwrap();
    assert_eq!(
        error,
        Error::Parse("syntax error at or near \"OIDS\"".to_string())
    );
}

#[test]
fn it_parses_CREATE_INDEX() {
    let result = summary(
        "CREATE INDEX testidx ON test USING btree (a, (lower(b) || upper(c))) WHERE pow(a, 2) > 25",
        0,
        -1,
    )
    .unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["test"]);
    assert_eq!(result.ddl_tables(), ["test"]);
    assert_eq!(result.statement_types(), ["IndexStmt"]);
    let call_functions: Vec<String> = sorted(result.call_functions()).collect();
    assert_eq!(call_functions, ["lower", "pow", "upper"]);
}

#[test]
fn it_parses_CREATE_SCHEMA() {
    let result = summary("CREATE SCHEMA IF NOT EXISTS test AUTHORIZATION joe", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.statement_types(), ["CreateSchemaStmt"]);
}

#[test]
fn it_parses_CREATE_VIEW() {
    let result = summary("CREATE VIEW myview AS SELECT * FROM mytab", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    let tables: Vec<String> = sorted(result.tables()).collect();
    assert_eq!(tables, ["mytab", "myview"]);
    assert_eq!(result.ddl_tables(), ["myview"]);
    assert_eq!(result.select_tables(), ["mytab"]);
    assert_eq!(result.statement_types(), ["ViewStmt", "SelectStmt"]);
}

#[test]
fn it_parses_REFRESH_MATERIALIZED_VIEW() {
    let result = summary("REFRESH MATERIALIZED VIEW myview", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["myview"]);
    assert_eq!(result.ddl_tables(), ["myview"]);
    assert_eq!(result.statement_types(), ["RefreshMatViewStmt"]);
}

#[test]
fn it_parses_CREATE_RULE() {
    let sql = "CREATE RULE shoe_ins_protect AS ON INSERT TO shoe DO INSTEAD NOTHING";
    let result = summary(sql, 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["shoe"]);
    assert_eq!(result.ddl_tables(), ["shoe"]);
    assert_eq!(result.statement_types(), ["RuleStmt"]);
}

#[test]
fn it_parses_CREATE_TRIGGER() {
    let sql = "CREATE TRIGGER check_update BEFORE UPDATE ON accounts FOR EACH ROW EXECUTE PROCEDURE check_account_update()";
    let result = summary(sql, 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["accounts"]);
    assert_eq!(result.ddl_tables(), ["accounts"]);
    assert_eq!(result.statement_types(), ["CreateTrigStmt"]);
}

#[test]
fn it_parses_DROP_SCHEMA() {
    let result = summary("DROP SCHEMA myschema", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.statement_types(), ["DropStmt"]);
}

#[test]
fn it_parses_DROP_VIEW() {
    let result = summary("DROP VIEW myview, myview2", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.statement_types(), ["DropStmt"]);
}

#[test]
fn it_parses_DROP_INDEX() {
    let result = summary("DROP INDEX CONCURRENTLY myindex", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.statement_types(), ["DropStmt"]);
}

#[test]
fn it_parses_DROP_RULE() {
    let result = summary("DROP RULE myrule ON mytable CASCADE", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["mytable"]);
    assert_eq!(result.ddl_tables(), ["mytable"]);
    assert_eq!(result.statement_types(), ["DropStmt"]);
}

#[test]
fn it_parses_DROP_TRIGGER() {
    let result = summary(
        "DROP TRIGGER IF EXISTS mytrigger ON mytable RESTRICT",
        0,
        -1,
    )
    .unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["mytable"]);
    assert_eq!(result.ddl_tables(), ["mytable"]);
    assert_eq!(result.statement_types(), ["DropStmt"]);
}

#[test]
fn it_parses_GRANT() {
    let result = summary("GRANT INSERT, UPDATE ON mytable TO myuser", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["mytable"]);
    assert_eq!(result.ddl_tables(), ["mytable"]);
    assert_eq!(result.statement_types(), ["GrantStmt"]);
}

#[test]
fn it_parses_REVOKE() {
    let result = summary("REVOKE admins FROM joe", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.statement_types(), ["GrantRoleStmt"]);
}

#[test]
fn it_parses_TRUNCATE() {
    let result = summary(r#"TRUNCATE bigtable, "fattable" RESTART IDENTITY"#, 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    let tables: Vec<String> = sorted(result.tables()).collect();
    let ddl_tables: Vec<String> = sorted(result.ddl_tables()).collect();
    assert_eq!(tables, ["bigtable", "fattable"]);
    assert_eq!(ddl_tables, ["bigtable", "fattable"]);
    assert_eq!(result.statement_types(), ["TruncateStmt"]);
}

#[test]
fn it_parses_WITH() {
    let result = summary(
        "WITH a AS (SELECT * FROM x WHERE x.y = $1 AND x.z = 1) SELECT * FROM a",
        0,
        -1,
    )
    .unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["x"]);
    assert_eq!(result.cte_names, ["a"]);
    assert_eq!(result.statement_types(), ["SelectStmt"]);
}

#[test]
fn it_parses_DROP_TYPE() {
    let result = summary("DROP TYPE IF EXISTS repack.pk_something", 0, -1).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.statement_types(), ["DropStmt"]);

    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.aliases.len(), 0);
    assert_eq!(result.cte_names.len(), 0);
    assert_eq!(result.functions.len(), 0);
    assert_eq!(result.filter_columns.len(), 0);
    assert!(result.truncated_query.is_empty());
}
