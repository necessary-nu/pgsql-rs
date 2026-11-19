use pg19_nodes::*;
use pg19_operations::fingerprint::{
    fingerprint_nodes, fingerprint_statements, FingerprintOptions, FINGERPRINT_VERSION,
};

fn reference(sql: &str, flags: u8) -> String {
    for line in include_str!("fingerprint-v3.jsonl").lines() {
        let vector: serde_json::Value = serde_json::from_str(line).unwrap();
        if vector["sql"] == sql && vector["options"] == flags {
            assert_eq!(vector["version"], FINGERPRINT_VERSION);
            return vector["hex"].as_str().unwrap().to_owned();
        }
    }
    panic!("missing PG19 fingerprint vector for {sql} with flags {flags}")
}

fn target(value: Node) -> Node {
    ResTarget {
        val: Some(Box::new(value)),
        ..Default::default()
    }
    .into()
}

fn column(name: &str) -> Node {
    ColumnRef {
        fields: vec![Node::String(name.into())],
        location: Location::UNKNOWN,
    }
    .into()
}

fn select(targets: &[&str], schema: Option<&str>, table: &str, alias: Option<&str>) -> RawStmt {
    let range = RangeVar {
        inh: true,
        relpersistence: b'p',
        relname: Some(table.into()),
        schemaname: schema.map(str::to_owned),
        alias: alias.map(|a| {
            Box::new(Alias {
                aliasname: Some(a.into()),
                ..Default::default()
            })
        }),
        ..Default::default()
    };
    let stmt = SelectStmt {
        from_clause: vec![range.into()],
        target_list: targets.iter().map(|name| target(column(name))).collect(),
        ..Default::default()
    };
    RawStmt::new(stmt.into(), Location::default())
}

fn digest(stmt: &RawStmt, opts: FingerprintOptions) -> String {
    fingerprint_statements(std::slice::from_ref(stmt), opts).to_string()
}

// [spec:pgq:req:fingerprint.digest/test]
// [spec:pgq:req:fingerprint.lists/test]
#[test]
fn pg19_version_three_vectors_and_list_order() {
    let opts = FingerprintOptions::default();
    assert_eq!(FINGERPRINT_VERSION, 3);
    for cols in [&["x", "y"][..], &["y", "x"][..], &["x", "x", "y"][..]] {
        let stmt = select(cols, None, "z", None);
        let hashed = fingerprint_statements(std::slice::from_ref(&stmt), opts);
        assert_eq!(hashed.0, 0x330267237da5535f);
        assert_eq!(
            hashed.to_string(),
            reference(&format!("SELECT {} FROM z", cols.join(", ")), 0)
        );
        assert_eq!(fingerprint_nodes(&[stmt.into()], opts), hashed);
    }
    let x = select(&["x"], None, "z", None);
    let y = select(&["y"], None, "z", None);
    assert_ne!(
        fingerprint_statements(&[x.clone(), y.clone()], opts),
        fingerprint_statements(&[y, x], opts)
    );
}

// [spec:pgq:req:fingerprint.aliases/test]
#[test]
fn alias_and_schema_modes_match_pg19() {
    let stmt = select(&["x"], Some("s"), "t", Some("a"));
    for (opts, bits) in [
        (FingerprintOptions::default(), 0),
        (
            FingerprintOptions {
                ignore_aliases: true,
                ..Default::default()
            },
            1,
        ),
        (
            FingerprintOptions {
                include_schema: true,
                ..Default::default()
            },
            2,
        ),
        (FingerprintOptions::PG17_COMPAT, 3),
    ] {
        assert_eq!(
            digest(&stmt, opts),
            reference("SELECT x FROM s.t AS a", bits)
        );
    }
}

// [spec:pgq:req:fingerprint.relation-digits/test]
#[test]
fn relation_digits_and_combinations_match_pg19() {
    let a = select(&["x"], None, "orders_2024_01", None);
    let b = select(&["x"], None, "orders_2024_02", None);
    let opts = FingerprintOptions::default();
    assert_eq!(
        digest(&a, opts),
        reference("SELECT x FROM orders_2024_01", 0)
    );
    assert_eq!(digest(&b, opts), digest(&a, opts));
    let full = FingerprintOptions {
        full_relation_name: true,
        ..FingerprintOptions::PG17_COMPAT
    };
    assert_eq!(
        digest(&a, full),
        reference("SELECT x FROM orders_2024_01", 19)
    );
    assert_ne!(digest(&b, full), digest(&a, full));
    let first = select(&["x"], None, "orders1", None);
    let second = select(&["x"], None, "orders2", None);
    assert_ne!(digest(&first, opts), digest(&second, opts));
}

#[test]
fn zero_null_and_parameters_omit_empty_children() {
    let opts = FingerprintOptions::default();
    for (sql, value) in ["SELECT NULL", "SELECT 0", "SELECT $1"].into_iter().zip([
        Node::AConst(AConst::default()),
        Node::AConst(AConst {
            val: Literal::Integer(0),
            location: Location::UNKNOWN,
        }),
        ParamRef {
            number: 1,
            ..Default::default()
        }
        .into(),
    ]) {
        let base = SelectStmt {
            target_list: vec![target(value)],
            ..Default::default()
        };
        assert_eq!(
            digest(&RawStmt::new(base.into(), Location::default()), opts),
            reference(sql, 0)
        );
    }
}

#[test]
fn utility_vectors_preserve_native_fields() {
    let opts = FingerprintOptions::default();
    let repack = RepackStmt {
        command: RepackCommand::REPACK_COMMAND_REPACK,
        usingindex: true,
        indexname: Some("i".into()),
        relation: Some(Box::new(VacuumRelation {
            relation: Some(Box::new(RangeVar {
                relname: Some("t".into()),
                inh: true,
                relpersistence: b'p',
                location: Location::from_pg(7).unwrap(),
                ..Default::default()
            })),
            ..Default::default()
        })),
        ..Default::default()
    };
    let wait = WaitStmt {
        lsn_literal: Some("0/16B6C50".into()),
        lsn_location: Location::from_pg(13).unwrap(),
        ..Default::default()
    };
    assert_eq!(
        digest(&RawStmt::new(repack.into(), Location::default()), opts),
        reference("REPACK t USING INDEX i", 0)
    );
    assert_eq!(
        digest(&RawStmt::new(wait.into(), Location::default()), opts),
        reference("WAIT FOR LSN '0/16B6C50'", 0)
    );
}
