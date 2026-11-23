use super::*;

#[test]
fn it_parses_multi_line_functions() {
    let sql = "CREATE OR REPLACE FUNCTION thing(parameter_thing text)
  RETURNS bigint AS
$BODY$
DECLARE
        local_thing_id BIGINT := 0;
BEGIN
        SELECT thing_id INTO local_thing_id FROM thing_map
        WHERE
                thing_map_field = parameter_thing
        ORDER BY 1 LIMIT 1;

        IF NOT FOUND THEN
                local_thing_id = 0;
        END IF;
        RETURN local_thing_id;
END;
$BODY$
  LANGUAGE plpgsql STABLE";
    let result = parse(sql, 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.functions(), ["thing"]);
    assert_eq!(result.ddl_functions(), ["thing"]);
    assert_eq!(result.call_functions().len(), 0);
    assert_eq!(result.statement_types(), ["CreateFunctionStmt"]);
    let stmt = cast!(result.protobuf.nodes()[0].0, NodeRef::CreateFunctionStmt);
    assert_debug_eq!(
        stmt,
        include_str!("../data/ast/multi_line_functions.snap").trim_end()
    );
}

#[test]
fn it_parses_table_functions() {
    let sql = "CREATE FUNCTION getfoo(int) RETURNS TABLE (f1 int) AS 'SELECT * FROM foo WHERE fooid = $1;' LANGUAGE SQL";
    let result = parse(sql, 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.functions(), ["getfoo"]);
    assert_eq!(result.ddl_functions(), ["getfoo"]);
    assert_eq!(result.call_functions().len(), 0);
    assert_eq!(result.statement_types(), ["CreateFunctionStmt"]);
}

#[test]
fn it_finds_called_functions() {
    let result = parse("SELECT testfunc(1);", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.functions(), ["testfunc"]);
    assert_eq!(result.ddl_functions().len(), 0);
    assert_eq!(result.call_functions(), ["testfunc"]);
    assert_eq!(result.statement_types(), ["SelectStmt"]);
}

#[test]
fn it_finds_functions_invoked_with_CALL() {
    let result = parse("CALL testfunc(1);", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.functions(), ["testfunc"]);
    assert_eq!(result.ddl_functions().len(), 0);
    assert_eq!(result.call_functions(), ["testfunc"]);
    assert_eq!(result.statement_types(), ["CallStmt"]);
}

#[test]
fn it_finds_dropped_functions() {
    let result = parse("DROP FUNCTION IF EXISTS testfunc(x integer);", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.functions(), ["testfunc"]);
    assert_eq!(result.ddl_functions(), ["testfunc"]);
    assert_eq!(result.call_functions().len(), 0);
    assert_eq!(result.statement_types(), ["DropStmt"]);
}

#[test]
fn it_finds_renamed_functions() {
    let result = parse("ALTER FUNCTION testfunc(integer) RENAME TO testfunc2;", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 0);
    let functions: Vec<String> = sorted(result.functions()).collect();
    let ddl_functions: Vec<String> = sorted(result.functions()).collect();
    assert_eq!(functions, ["testfunc", "testfunc2"]);
    assert_eq!(ddl_functions, ["testfunc", "testfunc2"]);
    assert_eq!(result.call_functions().len(), 0);
    assert_eq!(result.statement_types(), ["RenameStmt"]);
}

#[test]
fn it_separates_CTE_names_from_table_names() {
    let sql = "WITH cte_name AS (SELECT 1) SELECT * FROM table_name, cte_name";
    let result = parse(sql, 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["table_name"]);
    assert_eq!(result.select_tables(), ["table_name"]);
    assert_eq!(result.cte_names, ["cte_name"]);
    assert_eq!(result.statement_types(), ["SelectStmt"]);
}

#[test]
fn subqueries_without_from() {
    let query = "
        SELECT *
        FROM pg_catalog.pg_class c
        JOIN (
            SELECT 17650 AS oid
            UNION ALL
            SELECT 17663 AS oid
        ) vals ON c.oid = vals.oid
    ";
    let result = parse(query, 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["pg_catalog.pg_class"]);
    assert_eq!(result.select_tables(), ["pg_catalog.pg_class"]);
    assert_eq!(result.statement_types(), ["SelectStmt"]);
    // JOIN ON alone does not contribute filter columns in this query.
    assert!(result.filter_columns.is_empty());
}

#[test]
fn it_finds_tables_inside_casts() {
    let sql = "
        SELECT 1
        FROM   foo
        WHERE  x = any(cast(array(SELECT a FROM bar) as bigint[]))
            OR x = any(array(SELECT a FROM baz)::bigint[])
    ";
    let result = parse(sql, 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    let tables: Vec<String> = sorted(result.tables()).collect();
    assert_eq!(tables, ["bar", "baz", "foo"]);
    assert_eq!(result.functions().len(), 0);
    assert_eq!(result.call_functions().len(), 0);
    assert_eq!(result.statement_types(), ["SelectStmt"]);
}

#[test]
fn it_finds_functions_in_FROM_clause() {
    let sql = "SELECT * FROM my_custom_func()";
    let result = parse(sql, 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.functions(), ["my_custom_func"]);
    assert_eq!(result.call_functions(), ["my_custom_func"]);
    assert_eq!(result.statement_types(), ["SelectStmt"]);
}

#[test]
fn it_finds_functions_in_LATERAL_clause() {
    let sql = "
        SELECT *
        FROM unnest($1::text[]) AS a(x)
        LEFT OUTER JOIN LATERAL (
            SELECT json_build_object($2, z.z)
            FROM (
                SELECT *
                FROM (
                    SELECT row_to_json(
                        (SELECT * FROM (SELECT public.my_function(b) FROM public.c) d)
                    )
                ) e
            ) f
        ) AS g ON (1)
    ";
    let result = parse(sql, 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["public.c"]);
    let functions: Vec<String> = sorted(result.functions()).collect();
    let call_functions: Vec<String> = sorted(result.call_functions()).collect();
    assert_eq!(
        functions,
        [
            "json_build_object",
            "public.my_function",
            "row_to_json",
            "unnest"
        ]
    );
    assert_eq!(call_functions, functions);
    assert_eq!(result.statement_types(), ["SelectStmt"]);
}

#[test]
fn it_parses_INSERT() {
    let result = parse("insert into users(pk, name) values (1, 'bob');", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["users"]);

    let result = parse(
        "insert into users(pk, name) select pk, name from other_users;",
        0,
    )
    .unwrap();
    assert_eq!(result.warnings.len(), 0);
    let tables: Vec<String> = sorted(result.tables()).collect();
    assert_eq!(tables, ["other_users", "users"]);

    let sql = "
        with cte as (
            select pk, name from other_users
        )
        insert into users(pk, name) select * from cte;
    ";
    let result = parse(sql, 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    let tables: Vec<String> = sorted(result.tables()).collect();
    assert_eq!(tables, ["other_users", "users"]);
    assert_eq!(result.select_tables(), ["other_users"]);
    assert_eq!(result.dml_tables(), ["users"]);
    assert_eq!(result.cte_names, ["cte"]);
    assert_eq!(result.statement_types(), ["InsertStmt"]);
}

#[test]
fn it_parses_UPDATE() {
    let result = parse("update users set name = 'bob';", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["users"]);
    assert_eq!(result.statement_types(), ["UpdateStmt"]);

    let result = parse(
        "update users set name = (select name from other_users limit 1);",
        0,
    )
    .unwrap();
    assert_eq!(result.warnings.len(), 0);
    let tables: Vec<String> = sorted(result.tables()).collect();
    assert_eq!(tables, ["other_users", "users"]);
    assert_eq!(result.statement_types(), ["UpdateStmt"]);

    let sql = "
        with cte as (
            select name from other_users limit 1
        )
        update users set name = (select name from cte);
    ";
    let result = parse(sql, 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    let tables: Vec<String> = sorted(result.tables()).collect();
    assert_eq!(tables, ["other_users", "users"]);
    assert_eq!(result.select_tables(), ["other_users"]);
    assert_eq!(result.dml_tables(), ["users"]);
    assert_eq!(result.cte_names, ["cte"]);
    assert_eq!(result.statement_types(), ["UpdateStmt"]);

    let sql = "
        UPDATE users SET name = users_new.name
        FROM users_new
        INNER JOIN join_table ON join_table.user_id = new_users.id
        WHERE users.id = users_new.id
    ";
    let result = parse(sql, 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    let tables: Vec<String> = sorted(result.tables()).collect();
    let select_tables: Vec<String> = sorted(result.select_tables()).collect();
    assert_eq!(tables, ["join_table", "users", "users_new"]);
    assert_eq!(select_tables, ["join_table", "users_new"]);
    assert_eq!(result.dml_tables(), ["users"]);
    assert_eq!(result.statement_types(), ["UpdateStmt"]);
}

#[test]
fn it_parses_DELETE() {
    let result = parse("DELETE FROM users;", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["users"]);
    assert_eq!(result.dml_tables(), ["users"]);
    assert_eq!(result.statement_types(), ["DeleteStmt"]);

    let sql = "
        DELETE FROM users USING foo
        WHERE foo_id = foo.id AND foo.action = 'delete';
    ";
    let result = parse(sql, 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    let tables: Vec<String> = sorted(result.tables()).collect();
    assert_eq!(tables, ["foo", "users"]);
    assert_eq!(result.dml_tables(), ["users"]);
    assert_eq!(result.select_tables(), ["foo"]);
    assert_eq!(result.statement_types(), ["DeleteStmt"]);

    let sql = "
        DELETE FROM users
        WHERE foo_id IN (SELECT id FROM foo WHERE action = 'delete');
    ";
    let result = parse(sql, 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    let tables: Vec<String> = sorted(result.tables()).collect();
    assert_eq!(tables, ["foo", "users"]);
    assert_eq!(result.dml_tables(), ["users"]);
    assert_eq!(result.select_tables(), ["foo"]);
    assert_eq!(result.statement_types(), ["DeleteStmt"]);
}
