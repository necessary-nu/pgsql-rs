#![allow(non_snake_case)]
#![cfg(test)]

#[cfg(test)]
use itertools::sorted;

use pgsql_syntax::{
    parse,
    protobuf::{self, a_const::Val},
    Error, NodeEnum, NodeRef, TriggerType,
};

#[macro_use]
mod support;
use support::*;

#[path = "parse/references.rs"]
mod references;

#[test]
fn it_parses_simple_query() {
    let result = parse("SELECT 1", 0).unwrap();
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.statement_types(), ["SelectStmt"]);
}

#[test]
fn it_handles_errors() {
    let error = parse("CREATE RANDOM ix_test ON contacts.person;", 0)
        .err()
        .unwrap();
    assert_eq!(
        error,
        Error::Parse("syntax error at or near \"RANDOM\"".into())
    );

    let error = parse("SELECT 'ERR", 0).err().unwrap();
    assert_eq!(
        error,
        Error::Parse("unterminated quoted string at or near \"'ERR\"".into())
    );
}

#[test]
fn it_serializes_as_json() {
    let result = parse("SELECT 1 FROM pg_class", 0).unwrap();
    let json = serde_json::to_string(&result.protobuf);

    assert!(json.is_ok(), "Protobuf should be serializable: {json:?}");
}

#[test]
fn it_handles_recursion_error() {
    let query = "SELECT a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(a(b))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))";
    assert!(matches!(parse(query, 0), Err(Error::Decode(_))));
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
    let result = parse(query, 0).unwrap();
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
    let result = parse(query, 0).unwrap();
    let tables: Vec<String> = sorted(result.tables()).collect();
    let select_tables: Vec<String> = sorted(result.select_tables()).collect();
    assert_eq!(tables, ["snapshots", "system_snapshots"]);
    assert_eq!(select_tables, ["snapshots", "system_snapshots"]);
    assert_eq!(result.statement_types(), ["SelectStmt"]);
}

#[test]
fn it_parses_empty_queries() {
    let result = parse("-- nothing", 0).unwrap();
    assert_eq!(result.protobuf.nodes().len(), 0);
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.statement_types().len(), 0);
}

#[test]
fn it_parses_floats_with_leading_dot() {
    let result = parse("SELECT .1", 0).unwrap();
    let select = cast!(result.protobuf.nodes()[0].0, NodeRef::SelectStmt);
    let target = cast!(
        select.target_list[0].node.as_ref().unwrap(),
        NodeEnum::ResTarget
    );
    let a_const = cast!(
        target.val.as_ref().unwrap().node.as_ref().unwrap(),
        NodeEnum::AConst
    );
    let float = cast!(a_const.val.as_ref().unwrap(), Val::Fval);
    assert_eq!(float.fval, ".1");
    assert_eq!(a_const.location, 7);
}

#[test]
fn it_parses_bit_strings_hex_notation() {
    let result = parse("SELECT X'EFFF'", 0).unwrap();
    let select = cast!(result.protobuf.nodes()[0].0, NodeRef::SelectStmt);
    let target = cast!(
        select.target_list[0].node.as_ref().unwrap(),
        NodeEnum::ResTarget
    );
    let a_const = cast!(
        target.val.as_ref().unwrap().node.as_ref().unwrap(),
        NodeEnum::AConst
    );
    let bit_string = cast!(a_const.val.as_ref().unwrap(), Val::Bsval);
    assert_eq!(bit_string.bsval, "xEFFF");
    assert_eq!(a_const.location, 7);
}

#[test]
fn it_parses_ALTER_TABLE() {
    let result = parse("ALTER TABLE test ADD PRIMARY KEY (gid)", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["test"]);
    assert_eq!(result.ddl_tables(), ["test"]);
    assert_eq!(result.statement_types(), ["AlterTableStmt"]);
    let alter = cast!(result.protobuf.nodes()[0].0, NodeRef::AlterTableStmt);
    let cmd = cast!(
        alter.cmds[0].node.as_ref().unwrap(),
        NodeEnum::AlterTableCmd
    );
    assert_debug_eq!(cmd, include_str!("data/ast/alter_table.snap").trim_end());
}

#[test]
fn it_parses_SET() {
    let result = parse("SET statement_timeout=1", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.ddl_tables().len(), 0);
    assert_eq!(result.statement_types(), ["VariableSetStmt"]);
    let set = cast!(result.protobuf.nodes()[0].0, NodeRef::VariableSetStmt);
    let a_const = cast!(set.args[0].node.as_ref().unwrap(), NodeEnum::AConst);
    let val = cast!(a_const.val.as_ref().unwrap(), Val::Ival);
    assert_eq!(val.ival, 1);
    assert_eq!(a_const.location, 22);
}

#[test]
fn it_parses_SHOW() {
    let result = parse("SHOW work_mem", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.statement_types(), ["VariableShowStmt"]);
    let show = cast!(result.protobuf.nodes()[0].0, NodeRef::VariableShowStmt);
    assert_eq!(show.name, "work_mem");
}

#[test]
fn it_parses_COPY() {
    let result = parse("COPY test (id) TO stdout", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["test"]);
    assert_eq!(result.statement_types(), ["CopyStmt"]);
    let copy = cast!(result.protobuf.nodes()[0].0, NodeRef::CopyStmt);
    assert_debug_eq!(copy, include_str!("data/ast/copy.snap").trim_end());
}

#[test]
fn it_parses_DROP_TABLE() {
    let result = parse("drop table abc.test123 cascade", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["abc.test123"]);
    assert_eq!(result.ddl_tables(), ["abc.test123"]);
    assert_eq!(result.statement_types(), ["DropStmt"]);
    let drop = cast!(result.protobuf.nodes()[0].0, NodeRef::DropStmt);
    assert_eq!(
        protobuf::DropBehavior::try_from(drop.behavior).ok(),
        Some(protobuf::DropBehavior::DropCascade)
    );

    let result = parse("drop table abc.test123, test", 0).unwrap();
    let tables: Vec<String> = sorted(result.tables()).collect();
    let ddl_tables: Vec<String> = sorted(result.ddl_tables()).collect();
    assert_eq!(tables, ["abc.test123", "test"]);
    assert_eq!(ddl_tables, ["abc.test123", "test"]);
}

#[test]
fn it_parses_COMMIT() {
    let result = parse("COMMIT", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.statement_types(), ["TransactionStmt"]);
    let stmt = cast!(result.protobuf.nodes()[0].0, NodeRef::TransactionStmt);
    assert_eq!(
        protobuf::TransactionStmtKind::try_from(stmt.kind).ok(),
        Some(protobuf::TransactionStmtKind::TransStmtCommit)
    );
}

#[test]
fn it_parses_CHECKPOINT() {
    let result = parse("CHECKPOINT", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.statement_types(), ["CheckPointStmt"]);
    cast!(result.protobuf.nodes()[0].0, NodeRef::CheckPointStmt);
}

#[test]
fn it_parses_VACUUM() {
    let result = parse("VACUUM my_table", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["my_table"]);
    assert_eq!(result.ddl_tables(), ["my_table"]);
    assert_eq!(result.statement_types(), ["VacuumStmt"]);
    cast!(result.protobuf.nodes()[0].0, NodeRef::VacuumStmt);
}

#[test]
fn it_parses_MERGE() {
    let result = parse(
        "WITH cte AS (SELECT * FROM g.other_table CROSS JOIN p) MERGE INTO my_table USING cte ON (id=oid) WHEN MATCHED THEN UPDATE SET a=b WHEN NOT MATCHED THEN INSERT (id, a) VALUES (oid, b);", 0)
    .unwrap();
    assert_eq!(result.warnings.len(), 0);

    let select_tables: Vec<String> = sorted(result.select_tables()).collect();
    assert_eq!(select_tables, ["g.other_table", "p"]);

    let dml_tables: Vec<String> = sorted(result.dml_tables()).collect();
    assert_eq!(dml_tables, ["my_table"]);

    let all_tables: Vec<String> = sorted(result.tables()).collect();
    assert_eq!(all_tables, ["g.other_table", "my_table", "p"]);

    assert_eq!(result.cte_names, ["cte"]);
    assert_eq!(result.statement_types(), ["MergeStmt"]);

    cast!(result.protobuf.nodes()[0].0, NodeRef::MergeStmt);
}

#[test]
fn it_parses_EXPLAIN() {
    let result = parse("EXPLAIN DELETE FROM test", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["test"]);
    assert_eq!(result.statement_types(), ["ExplainStmt"]);
    let stmt = cast!(result.protobuf.nodes()[0].0, NodeRef::ExplainStmt);
    cast!(
        stmt.query.as_ref().unwrap().node.as_ref().unwrap(),
        NodeEnum::DeleteStmt
    );
}

#[test]
fn it_parses_SELECT_INTO() {
    let result = parse("CREATE TEMP TABLE test AS SELECT 1", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["test"]);
    assert_eq!(result.ddl_tables(), ["test"]);
    assert_eq!(result.statement_types(), ["CreateTableAsStmt"]);
    let stmt = cast!(result.protobuf.nodes()[0].0, NodeRef::CreateTableAsStmt);
    let select = cast!(
        stmt.query.as_ref().unwrap().node.as_ref().unwrap(),
        NodeEnum::SelectStmt
    );
    let target = cast!(
        select.target_list[0].node.as_ref().unwrap(),
        NodeEnum::ResTarget
    );
    let a_const = cast!(
        target.val.as_ref().unwrap().node.as_ref().unwrap(),
        NodeEnum::AConst
    );
    let val = cast!(a_const.val.as_ref().unwrap(), Val::Ival);
    assert_eq!(val.ival, 1);
    let into = stmt.into.as_ref().unwrap();
    assert_debug_eq!(into, include_str!("data/ast/select_into.snap").trim_end());
}

#[test]
fn it_parses_LOCK() {
    let result = parse(
        "LOCK TABLE public.schema_migrations IN ACCESS SHARE MODE",
        0,
    )
    .unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["public.schema_migrations"]);
    assert_eq!(result.statement_types(), ["LockStmt"]);
    let stmt = cast!(result.protobuf.nodes()[0].0, NodeRef::LockStmt);
    assert_eq!(stmt.mode, 1);
}

#[test]
fn it_parses_CREATE_TABLE() {
    let result = parse("CREATE TABLE test (a int4)", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["test"]);
    assert_eq!(result.ddl_tables(), ["test"]);
    assert_eq!(result.statement_types(), ["CreateStmt"]);
    let stmt = cast!(result.protobuf.nodes()[0].0, NodeRef::CreateStmt);
    let column = cast!(
        stmt.table_elts[0].node.as_ref().unwrap(),
        NodeEnum::ColumnDef
    );
    assert_debug_eq!(
        column,
        include_str!("data/ast/create_table.snap").trim_end()
    );
}

#[test]
fn it_parses_CREATE_TABLE_AS() {
    let result = parse("CREATE TABLE foo AS SELECT * FROM bar;", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    let tables: Vec<String> = sorted(result.tables()).collect();
    assert_eq!(tables, ["bar", "foo"]);
    assert_eq!(result.ddl_tables(), ["foo"]);
    assert_eq!(result.select_tables(), ["bar"]);
    assert_eq!(result.statement_types(), ["CreateTableAsStmt"]);

    let sql = "CREATE TABLE foo AS SELECT id FROM bar UNION SELECT id from baz;";
    let result = parse(sql, 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    let tables: Vec<String> = sorted(result.tables()).collect();
    let select_tables: Vec<String> = sorted(result.select_tables()).collect();
    assert_eq!(tables, ["bar", "baz", "foo"]);
    assert_eq!(result.ddl_tables(), ["foo"]);
    assert_eq!(select_tables, ["bar", "baz"]);
    assert_eq!(result.statement_types(), ["CreateTableAsStmt"]);
}

#[test]
fn it_fails_to_parse_CREATE_TABLE_WITH_OIDS() {
    let error = parse("CREATE TABLE test (a int4) WITH OIDS", 0)
        .err()
        .unwrap();
    assert_eq!(
        error,
        Error::Parse("syntax error at or near \"OIDS\"".to_string())
    );
}

#[test]
fn it_parses_CREATE_INDEX() {
    let result = parse(
        "CREATE INDEX testidx ON test USING btree (a, (lower(b) || upper(c))) WHERE pow(a, 2) > 25",
        0,
    )
    .unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["test"]);
    assert_eq!(result.ddl_tables(), ["test"]);
    assert_eq!(result.statement_types(), ["IndexStmt"]);
    let call_functions: Vec<String> = sorted(result.call_functions()).collect();
    assert_eq!(call_functions, ["lower", "pow", "upper"]);
    let stmt = cast!(result.protobuf.nodes()[0].0, NodeRef::IndexStmt);
    assert_eq!(stmt.idxname, "testidx".to_string());
}

#[test]
fn it_parses_CREATE_SCHEMA() {
    let result = parse("CREATE SCHEMA IF NOT EXISTS test AUTHORIZATION joe", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.statement_types(), ["CreateSchemaStmt"]);
    let stmt = cast!(result.protobuf.nodes()[0].0, NodeRef::CreateSchemaStmt);
    assert_debug_eq!(stmt, include_str!("data/ast/create_schema.snap").trim_end());
}

#[test]
fn it_parses_CREATE_VIEW() {
    let result = parse("CREATE VIEW myview AS SELECT * FROM mytab", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    let tables: Vec<String> = sorted(result.tables()).collect();
    assert_eq!(tables, ["mytab", "myview"]);
    assert_eq!(result.ddl_tables(), ["myview"]);
    assert_eq!(result.select_tables(), ["mytab"]);
    assert_eq!(result.statement_types(), ["ViewStmt"]);
    let stmt = cast!(result.protobuf.nodes()[0].0, NodeRef::ViewStmt);
    assert_debug_eq!(stmt, include_str!("data/ast/create_view.snap").trim_end());
}

#[test]
fn it_parses_REFRESH_MATERIALIZED_VIEW() {
    let result = parse("REFRESH MATERIALIZED VIEW myview", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["myview"]);
    assert_eq!(result.ddl_tables(), ["myview"]);
    assert_eq!(result.statement_types(), ["RefreshMatViewStmt"]);
    cast!(result.protobuf.nodes()[0].0, NodeRef::RefreshMatViewStmt);
}

#[test]
fn it_parses_CREATE_RULE() {
    let sql = "CREATE RULE shoe_ins_protect AS ON INSERT TO shoe DO INSTEAD NOTHING";
    let result = parse(sql, 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["shoe"]);
    assert_eq!(result.ddl_tables(), ["shoe"]);
    assert_eq!(result.statement_types(), ["RuleStmt"]);
    let stmt = cast!(result.protobuf.nodes()[0].0, NodeRef::RuleStmt);
    assert_eq!(stmt.rulename, "shoe_ins_protect");
    assert_eq!(
        protobuf::CmdType::try_from(stmt.event).ok(),
        Some(protobuf::CmdType::CmdInsert)
    );
}

#[test]
fn it_parses_CREATE_TRIGGER() {
    let sql = "CREATE TRIGGER check_update BEFORE UPDATE ON accounts FOR EACH ROW EXECUTE PROCEDURE check_account_update()";
    let result = parse(sql, 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["accounts"]);
    assert_eq!(result.ddl_tables(), ["accounts"]);
    assert_eq!(result.statement_types(), ["CreateTrigStmt"]);
    let stmt = cast!(result.protobuf.nodes()[0].0, NodeRef::CreateTrigStmt);
    let func = cast!(stmt.funcname[0].node.as_ref().unwrap(), NodeEnum::String);
    assert_eq!(func.sval, "check_account_update");
    assert_eq!(
        TriggerType::try_from(stmt.timing).ok(),
        Some(TriggerType::Before)
    );
    assert_eq!(
        TriggerType::try_from(stmt.events).ok(),
        Some(TriggerType::Update)
    );
}

#[test]
fn it_parses_DROP_SCHEMA() {
    let result = parse("DROP SCHEMA myschema", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.statement_types(), ["DropStmt"]);
    let stmt = cast!(result.protobuf.nodes()[0].0, NodeRef::DropStmt);
    assert_debug_eq!(stmt, include_str!("data/ast/drop_schema.snap").trim_end());
}

#[test]
fn it_parses_DROP_VIEW() {
    let result = parse("DROP VIEW myview, myview2", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.statement_types(), ["DropStmt"]);
    let stmt = cast!(result.protobuf.nodes()[0].0, NodeRef::DropStmt);
    assert_debug_eq!(stmt, include_str!("data/ast/drop_view.snap").trim_end());
}

#[test]
fn it_parses_DROP_INDEX() {
    let result = parse("DROP INDEX CONCURRENTLY myindex", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.statement_types(), ["DropStmt"]);
    let stmt = cast!(result.protobuf.nodes()[0].0, NodeRef::DropStmt);
    assert_debug_eq!(stmt, include_str!("data/ast/drop_index.snap").trim_end());
}

#[test]
fn it_parses_DROP_RULE() {
    let result = parse("DROP RULE myrule ON mytable CASCADE", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["mytable"]);
    assert_eq!(result.ddl_tables(), ["mytable"]);
    assert_eq!(result.statement_types(), ["DropStmt"]);
    let stmt = cast!(result.protobuf.nodes()[0].0, NodeRef::DropStmt);
    assert_debug_eq!(stmt, include_str!("data/ast/drop_rule.snap").trim_end());
}

#[test]
fn it_parses_DROP_TRIGGER() {
    let result = parse("DROP TRIGGER IF EXISTS mytrigger ON mytable RESTRICT", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["mytable"]);
    assert_eq!(result.ddl_tables(), ["mytable"]);
    assert_eq!(result.statement_types(), ["DropStmt"]);
    let stmt = cast!(result.protobuf.nodes()[0].0, NodeRef::DropStmt);
    assert_debug_eq!(stmt, include_str!("data/ast/drop_trigger.snap").trim_end());
}

#[test]
fn it_parses_GRANT() {
    let result = parse("GRANT INSERT, UPDATE ON mytable TO myuser", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["mytable"]);
    assert_eq!(result.ddl_tables(), ["mytable"]);
    assert_eq!(result.statement_types(), ["GrantStmt"]);
    let stmt = cast!(result.protobuf.nodes()[0].0, NodeRef::GrantStmt);
    assert_debug_eq!(stmt, include_str!("data/ast/grant.snap").trim_end());
}

#[test]
fn it_parses_REVOKE() {
    let result = parse("REVOKE admins FROM joe", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables().len(), 0);
    assert_eq!(result.statement_types(), ["GrantRoleStmt"]);
    let stmt = cast!(result.protobuf.nodes()[0].0, NodeRef::GrantRoleStmt);
    assert_debug_eq!(stmt, include_str!("data/ast/revoke.snap").trim_end());
}

#[test]
fn it_parses_TRUNCATE() {
    let result = parse(r#"TRUNCATE bigtable, "fattable" RESTART IDENTITY"#, 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    let tables: Vec<String> = sorted(result.tables()).collect();
    let ddl_tables: Vec<String> = sorted(result.ddl_tables()).collect();
    assert_eq!(tables, ["bigtable", "fattable"]);
    assert_eq!(ddl_tables, ["bigtable", "fattable"]);
    assert_eq!(result.statement_types(), ["TruncateStmt"]);
    let stmt = cast!(result.protobuf.nodes()[0].0, NodeRef::TruncateStmt);
    assert_debug_eq!(stmt, include_str!("data/ast/truncate.snap").trim_end());
}

#[test]
fn it_parses_WITH() {
    let result = parse(
        "WITH a AS (SELECT * FROM x WHERE x.y = $1 AND x.z = 1) SELECT * FROM a",
        0,
    )
    .unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.tables(), ["x"]);
    assert_eq!(result.cte_names, ["a"]);
    assert_eq!(result.statement_types(), ["SelectStmt"]);
}

#[test]
fn it_parses_DROP_TYPE() {
    let result = parse("DROP TYPE IF EXISTS repack.pk_something", 0).unwrap();
    assert_eq!(result.warnings.len(), 0);
    assert_eq!(result.statement_types(), ["DropStmt"]);
    assert_debug_eq!(
        result.protobuf.nodes()[0].0,
        include_str!("data/ast/drop_type.snap").trim_end()
    );
}
