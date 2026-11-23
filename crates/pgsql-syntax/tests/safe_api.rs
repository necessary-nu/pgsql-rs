use pgsql_syntax::{
    analyze, decode_parse_result, detailed, parse, protobuf, summary, visit, visit_mut, NodeRef,
    NodeRefMut, ParseConfig, ParseMode, ParserOptions, PG_VERSION_NUM,
};
use prost::Message;

fn sorted<T: Ord>(mut items: Vec<T>) -> Vec<T> {
    items.sort();
    items
}

// [spec:pgq:req:api.pg-query+2/test]
#[test]
fn ctes_obey_query_block_and_declaration_scope() {
    let cases: &[(&str, &[&str], &[&str])] = &[
        ("WITH t AS (SELECT 1) SELECT * FROM t; SELECT * FROM t", &["t"], &[]),
        ("WITH t AS (SELECT * FROM t) SELECT * FROM t", &["t"], &[]),
        ("WITH a AS (SELECT * FROM b), b AS (SELECT * FROM a) SELECT * FROM b", &["b"], &[]),
        ("WITH RECURSIVE a AS (SELECT * FROM b), b AS (SELECT * FROM base) SELECT * FROM a", &["base"], &[]),
        ("SELECT * FROM (WITH t AS (SELECT 1) SELECT * FROM t) q, t", &["t"], &[]),
        ("WITH t AS (SELECT 1) SELECT * FROM (WITH t AS (SELECT * FROM t) SELECT * FROM t) q, public.t", &["public.t"], &[]),
        ("WITH t AS (SELECT 1) UPDATE t SET a = 2 FROM t AS src", &[], &["t"]),
        ("WITH t AS (SELECT 1) INSERT INTO t SELECT * FROM t", &[], &["t"]),
        ("WITH t AS (SELECT 1) DELETE FROM t USING t AS src", &[], &["t"]),
        ("WITH t AS (SELECT 1) MERGE INTO t USING src ON true WHEN MATCHED THEN DO NOTHING", &["src"], &["t"]),
    ];
    for &(sql, reads, writes) in cases {
        let parsed = parse(sql, 0).unwrap();
        assert_eq!(sorted(parsed.select_tables()), reads, "{sql}");
        assert_eq!(sorted(parsed.dml_tables()), writes, "{sql}");
        for report in [
            summary(sql, 0, -1).unwrap(),
            analyze(&parsed.protobuf).unwrap(),
        ] {
            assert_eq!(sorted(report.select_tables()), reads, "{sql}");
            assert_eq!(sorted(report.dml_tables()), writes, "{sql}");
        }
    }
}

#[test]
fn expression_and_subquery_filter_context() {
    let sql = "INSERT INTO dst SELECT x FROM a WHERE EXISTS (SELECT unrelated FROM inside_table WHERE inner_col) AND outer_col LIMIT (SELECT max(n) FROM b) RETURNING f(id), (SELECT g(y) FROM c)";
    let parsed = parse(sql, 0).unwrap();
    let report = summary(sql, 0, -1).unwrap();
    assert_eq!(
        sorted(parsed.select_tables()),
        ["a", "b", "c", "inside_table"]
    );
    assert_eq!(parsed.dml_tables(), ["dst"]);
    assert_eq!(sorted(parsed.functions()), ["f", "g", "max"]);
    assert_eq!(sorted(report.tables()), sorted(parsed.tables()));
    assert_eq!(sorted(report.functions()), sorted(parsed.functions()));
    let filters: Vec<_> = parsed
        .filter_columns
        .iter()
        .map(|(_, c)| c.as_str())
        .collect();
    assert_eq!(sorted(filters), ["inner_col", "outer_col"]);
    assert_eq!(report.protobuf, analyze(&parsed.protobuf).unwrap().protobuf);
}

#[test]
fn visitor_follows_edited_lists() {
    let mut parsed = parse("INSERT INTO original VALUES (1) RETURNING f(2), g(3)", 0).unwrap();
    let mut before = Vec::new();
    visit(&parsed.protobuf, |record| {
        if let NodeRef::Integer(i) = record {
            before.push(i.ival);
        }
    });
    assert_eq!(before, [1, 2, 3]);

    let mut changed = 0;
    visit_mut(&mut parsed.protobuf, |record| match record {
        NodeRefMut::RangeVar(r) => r.relname = "edited".into(),
        NodeRefMut::ReturningClause(r) => {
            r.exprs.truncate(1);
        }
        NodeRefMut::Integer(i) => {
            i.ival += 10;
            changed += 1;
        }
        _ => {}
    });
    assert_eq!(changed, 2);
    assert_eq!(parsed.dml_tables(), ["original"]);
    let fresh = analyze(&parsed.protobuf).unwrap();
    assert_eq!(fresh.dml_tables(), ["edited"]);
    assert_eq!(fresh.functions(), ["f"]);
    assert_eq!(
        parsed.deparse(Default::default()).unwrap(),
        "INSERT INTO edited VALUES (11) RETURNING f(12)"
    );
}

#[test]
fn visits_literal_variants_and_replacement_children() {
    let mut parsed = parse("SELECT 0", 0).unwrap();
    let replacement = parse("SELECT 1, 2.5, true, 'text', B'01', NULL", 0).unwrap();
    let mut new_stmt = replacement.protobuf.stmts.into_iter().next().unwrap().stmt;
    let mut literals = Vec::new();
    visit_mut(&mut parsed.protobuf, |record| match record {
        NodeRefMut::RawStmt(stmt) => stmt.stmt = new_stmt.take(),
        NodeRefMut::Integer(n) => literals.push(n.ival.to_string()),
        NodeRefMut::Float(n) => literals.push(n.fval.clone()),
        NodeRefMut::Boolean(n) => literals.push(n.boolval.to_string()),
        NodeRefMut::String(n) => literals.push(n.sval.clone()),
        NodeRefMut::BitString(n) => literals.push(n.bsval.clone()),
        NodeRefMut::AConst(n) if n.isnull => literals.push("NULL".into()),
        _ => {}
    });
    assert_eq!(literals, ["1", "2.5", "true", "text", "b01", "NULL"]);
}

#[test]
fn external_wire_import_requires_exact_pg19_schema() {
    let mut tree = parse("SELECT 1", 0).unwrap().protobuf;
    assert_eq!(tree.version, PG_VERSION_NUM);
    assert_eq!(decode_parse_result(&tree.encode_to_vec()).unwrap(), tree);
    for version in [180000, 200000, -1] {
        tree.version = version;
        assert!(decode_parse_result(&tree.encode_to_vec()).is_err());
        assert!(analyze(&tree).is_err());
        assert!(tree.deparse(Default::default()).is_err());
        assert!(pgsql_syntax::truncate(&tree, 80).is_err());
        assert!(pgsql_syntax::ParseResult::try_new(tree.clone(), String::new()).is_err());
    }
    tree.version = 0;
    assert!(decode_parse_result(&tree.encode_to_vec()).is_err());
    assert_eq!(tree.deparse(Default::default()).unwrap(), "SELECT 1");
    assert!(analyze(&protobuf::ParseResult::default())
        .unwrap()
        .tables()
        .is_empty());
    assert!(decode_parse_result(&[0xff]).is_err());
}

#[test]
fn typed_modes_and_options_match_the_compatibility_api() {
    for (mode, bits, sql) in [
        (ParseMode::Sql, 0, "SELECT 1"),
        (ParseMode::TypeName, 1, "varchar(4)"),
        (ParseMode::PlPgSqlExpr, 2, "2 + 3"),
        (ParseMode::PlPgSqlAssign1, 3, "x := 5"),
        (ParseMode::PlPgSqlAssign2, 4, "x.y := 5"),
        (ParseMode::PlPgSqlAssign3, 5, "x.y.z := 5"),
    ] {
        assert_eq!(
            parse(sql, mode).unwrap().protobuf,
            parse(sql, bits).unwrap().protobuf
        );
        assert_eq!(
            ParserOptions::from(ParseConfig {
                mode,
                disable_backslash_quote: true
            })
            .bits(),
            bits | 16
        );
    }
    let config = ParseConfig {
        disable_backslash_quote: true,
        ..Default::default()
    };
    assert!(detailed::parse("SELECT E'a\\'b'", config).is_err());
}

#[test]
fn sqlstate_cursor_and_routine_context() {
    let sql = "SELECT 'é', )";
    let pos = sql.chars().count();
    for error in [
        detailed::parse(sql, 0).unwrap_err(),
        detailed::normalize(sql).unwrap_err(),
        detailed::summary(sql, 0, -1).unwrap_err(),
        detailed::fingerprint(sql, 0, 0).err().unwrap(),
        detailed::is_utility_stmt(sql).unwrap_err(),
        detailed::split_with_parser(sql).unwrap_err(),
    ] {
        let detailed::Error::Postgres {
            diagnostic,
            source_text,
            ..
        } = error
        else {
            panic!("{error:?}")
        };
        assert_eq!(source_text, detailed::Source::Sql);
        assert_eq!(diagnostic.sqlstate, "42601");
        assert_eq!(diagnostic.cursor_position().unwrap().get(), pos);
    }
    for error in [
        detailed::scan("SELECT 'é").unwrap_err(),
        detailed::deparse_comments_for_query("SELECT 'é").unwrap_err(),
    ] {
        assert!(
            matches!(error, detailed::Error::Postgres { operation: detailed::Operation::Scan, diagnostic, .. } if diagnostic.position == 8)
        );
    }
    let sql = "DO $$ BEGIN SELECT 4 INTO missing; END $$";
    let detailed::Error::Postgres {
        diagnostic,
        source_text,
        context,
        ..
    } = detailed::parse_plpgsql(sql).unwrap_err()
    else {
        panic!("missing diagnostic")
    };
    assert_eq!(source_text, detailed::Source::RoutineBody);
    assert_eq!(diagnostic.sqlstate, "42601");
    assert!(diagnostic.cursor_position().is_some());
    assert!(context.unwrap().contains("near line 1"));
    assert_eq!(
        pgsql_syntax::parse_plpgsql(sql).unwrap_err(),
        pgsql_syntax::Error::Parse(diagnostic.message)
    );
}
