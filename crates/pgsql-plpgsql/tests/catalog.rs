use pgsql_nodes::{Node, TypeName};
use pgsql_plpgsql::{Builtin, CompileError, DataType, EmbeddedSql, NativeSql, ParseMode, BUILTINS};

fn name(parts: &[&str]) -> TypeName {
    TypeName {
        names: parts.iter().map(|s| Node::String((*s).into())).collect(),
        ..TypeName::default()
    }
}

#[test]
fn pg19_builtin_types_and_array_oids() {
    assert_eq!(BUILTINS.len(), 197);
    assert_eq!(Builtin::named("oid8").unwrap().oid, 6437);
    for &t in BUILTINS {
        assert_eq!(Builtin::by_oid(t.oid), Some(t));
        if t.array_oid != 0 {
            assert!(Builtin::by_oid(t.array_oid).is_some());
        }
    }
    let mut oid8 = name(&["pg_catalog", "oid8"]);
    oid8.array_bounds = vec![Node::Integer(-1)];
    assert_eq!(
        Builtin::resolve(&oid8).unwrap().datatype(),
        DataType::Scalar("_oid8".into())
    );
    assert_eq!(
        Builtin::resolve(&name(&["public", "text"]))
            .unwrap()
            .datatype(),
        DataType::Record
    );
    assert_eq!(
        Builtin::resolve(&name(&["unknown_type"]))
            .unwrap()
            .datatype(),
        DataType::Record
    );
    assert!(
        matches!(Builtin::resolve(&name(&["pg_catalog","unavailable"])),Err(CompileError::Diagnostic(e)) if e.sqlstate=="42704")
    );
}

#[test]
fn native_adapter_uses_reference_start_modes() {
    let mut sql = NativeSql::default();
    assert_eq!(
        sql.datatype("\"foo\".bar%TYPE"),
        Ok(DataType::Scalar("foo.bar%TYPE".into()))
    );
    assert_eq!(
        sql.datatype("my_data%ROWTYPE"),
        Ok(DataType::Scalar("my_data%rowtype".into()))
    );
    for (src, mode) in [
        ("SELECT 1 + 2", ParseMode::Sql),
        ("x := 1", ParseMode::PlPgSqlAssign1),
    ] {
        let reference = pgsql_grammar::parse(src, mode, sql.options);
        match reference {
            Ok(_) => assert_eq!(sql.validate(src, mode), Ok(())),
            Err(e) => assert_eq!(sql.validate(src, mode), Err(e.into())),
        }
    }
}
