use pgsql_syntax::{parse, protobuf, NodeEnum, NodeMut, ParserOptions};
use prost::Message;

// [spec:pgq:req:api.pg-query+2/test]
#[test]
fn edit_and_deparse_pg19_wire_tree() {
    let mut parsed = parse("REPACK t USING INDEX i", 0).unwrap();
    assert_eq!(
        parsed.protobuf.encode_to_vec(),
        include_bytes!("../../pgsql-codec/tests/repack.pb")
    );
    assert_eq!(parsed.ddl_tables(), ["t"]);
    assert_eq!(parsed.statement_types(), ["RepackStmt"]);
    let mut edited = false;
    // The relation stays in its original allocation for the whole traversal.
    unsafe {
        for (node, _, _) in parsed.protobuf.nodes_mut() {
            if let NodeMut::RangeVar(p) = node {
                (*p).relname = "accounts".into();
                edited = true;
            }
        }
    }
    assert!(edited);
    let external =
        protobuf::ParseResult::decode(parsed.protobuf.encode_to_vec().as_slice()).unwrap();
    assert_eq!(
        external.deparse(Default::default()).unwrap(),
        "REPACK accounts USING INDEX i"
    );
    assert_eq!(
        pgsql_syntax::truncate(&external, 80).unwrap(),
        "REPACK accounts USING INDEX i"
    );
    assert_eq!(
        parse("WAIT FOR LSN '0/1'", 0).unwrap().statement_types(),
        ["WaitStmt"]
    );
}

#[test]
fn alternate_start_rules_return_typed_trees() {
    let t = parse("character varying(32)", ParserOptions::TYPE_NAME).unwrap();
    assert!(matches!(
        t.protobuf.stmts[0].stmt.as_ref().unwrap().node,
        Some(NodeEnum::TypeName(_))
    ));
    let assignment = parse("rec.field := 42", ParserOptions::PLPGSQL_ASSIGN2).unwrap();
    assert_eq!(assignment.statement_types(), ["PLAssignStmt"]);
    let Some(NodeEnum::PlassignStmt(a)) = assignment.protobuf.stmts[0]
        .stmt
        .as_ref()
        .unwrap()
        .node
        .as_ref()
    else {
        panic!("expected assignment")
    };
    assert_eq!(a.nnames, 2);
    assert!(parse(
        "SELECT E'foo\\'bar'",
        ParserOptions::DISABLE_BACKSLASH_QUOTE
    )
    .is_err());
}

#[test]
fn rejects_nul_inputs_consistently() {
    let sql = "SELECT 1\0; SELECT 2";
    for error in [
        parse(sql, 0).unwrap_err(),
        pgsql_syntax::normalize(sql).unwrap_err(),
        pgsql_syntax::scan(sql).unwrap_err(),
        pgsql_syntax::split_with_parser(sql).unwrap_err(),
        pgsql_syntax::split_with_scanner(sql).unwrap_err(),
        pgsql_syntax::is_utility_stmt(sql).unwrap_err(),
        pgsql_syntax::fingerprint(sql, 0, 0).err().unwrap(),
        pgsql_syntax::parse_plpgsql(sql).unwrap_err(),
        pgsql_syntax::summary(sql, 0, -1).unwrap_err(),
        pgsql_syntax::deparse_comments_for_query(sql).unwrap_err(),
    ] {
        assert!(matches!(error, pgsql_syntax::Error::Conversion(ref e) if e.nul_position() == 8));
    }
}

#[test]
fn legacy_index_walk_keeps_edge_order() {
    use pgsql_syntax::{Context, NodeRef};

    let mut tree = parse("CREATE INDEX ix ON t ((f(a))) WHERE b", 0)
        .unwrap()
        .protobuf;
    let records = tree.nodes();
    assert!(matches!(
        records.as_slice(),
        [
            (NodeRef::IndexStmt(_), 1, Context::None, false),
            (NodeRef::RangeVar(_), 2, Context::DDL, false),
            (NodeRef::IndexElem(_), 2, Context::DDL, false),
            (NodeRef::FuncCall(_), 2, Context::DDL, false),
            (NodeRef::ColumnRef(_), 2, Context::DDL, true),
            (NodeRef::ColumnRef(_), 3, Context::DDL, false),
        ]
    ));
    // Only inspect the tags and depths; all pointers still belong to the live tree.
    let mutable = unsafe { tree.nodes_mut() };
    assert!(matches!(
        mutable.as_slice(),
        [
            (NodeMut::IndexStmt(_), 1, Context::None),
            (NodeMut::RangeVar(_), 2, Context::DDL),
            (NodeMut::FuncCall(_), 2, Context::DDL),
            (NodeMut::ColumnRef(_), 3, Context::DDL),
        ]
    ));
}
