use pg19_codec::{node_json, statements_json, EncodeError};
use pg19_nodes::*;

fn loc(n: usize) -> Location {
    Location::from_byte_offset(n).unwrap()
}

// [spec:pgq:req:codec.json-envelope/test]
#[test]
fn empty_stream_and_ordered_pg19_statements() {
    assert_eq!(
        statements_json(&[]).unwrap(),
        r#"{"version":190000,"stmts":[]}"#
    );
    let range = RangeVar {
        relname: Some("t".into()),
        inh: true,
        relpersistence: b'p',
        location: loc(7),
        ..Default::default()
    };
    let repack = RepackStmt {
        command: RepackCommand::REPACK_COMMAND_REPACK,
        relation: Some(Box::new(VacuumRelation {
            relation: Some(Box::new(range)),
            ..Default::default()
        })),
        indexname: Some("i".into()),
        usingindex: true,
        ..Default::default()
    };
    let wait = WaitStmt {
        lsn_literal: Some("0/16B6C50".into()),
        lsn_location: loc(37),
        ..Default::default()
    };
    let mut first = RawStmt::new(repack.into(), loc(0));
    first.terminate_at(loc(22)).unwrap();
    let stmts = [first, RawStmt::new(wait.into(), loc(24))];
    assert_eq!(
        statements_json(&stmts).unwrap(),
        include_str!("pg19.json").trim_end()
    );
}

// [spec:pgq:req:codec.json-fields/test]
#[test]
fn constants_preserve_variants_and_compatibility_spelling() {
    let literals = [
        (16, Literal::Integer(0)),
        (19, Literal::Null),
        (25, Literal::Boolean(false)),
        (32, Literal::String("".into())),
        (36, Literal::String("<é>".into())),
        (44, Literal::BitString("b10".into())),
        (51, Literal::Float("0.50".into())),
        (57, Literal::Integer(7)),
    ];
    let target_list = literals
        .into_iter()
        .map(|(offset, val)| {
            let location = loc(offset);
            ResTarget {
                location,
                val: Some(Box::new(AConst { val, location }.into())),
                ..Default::default()
            }
            .into()
        })
        .collect();
    let select = SelectStmt {
        distinct_clause: vec![Node::Null],
        target_list,
        ..Default::default()
    };
    let mut stmt = RawStmt::new(select.into(), loc(0));
    stmt.terminate_at(loc(58)).unwrap();
    let result = statements_json(&[stmt]).unwrap();
    assert_eq!(result, include_str!("literals.json").trim_end());
}

#[test]
fn depth_error_leaves_later_conversions_usable() {
    let mut node = Node::String("bottom".into());
    for _ in 0..160 {
        node = Node::from(vec![node]);
    }
    assert_eq!(node_json(&node), Err(EncodeError::DepthLimit));
    assert_eq!(
        node_json(&Node::Boolean(false)).unwrap(),
        r#"{"Boolean":{"boolval":false}}"#
    );
}

#[test]
fn lists_and_unknown_locations_preserve_json_representation() {
    let root = Node::from(vec![
        Node::Null,
        Node::Integer(0),
        Node::IntList(vec![1, -1]),
        Node::OidList(vec![42]),
        Node::String("\n\t\u{0001}\"\\".into()),
        Node::AConst(AConst {
            val: Literal::Null,
            location: Location::UNKNOWN,
        }),
    ]);
    let result = node_json(&root).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&result).unwrap(),
        serde_json::json!({
            "List":{"items":[{},{"Integer":{}},{"IntList":{"items":[1,-1]}},{"OidList":{"items":[42]}},
            {"String":{"sval":"\n\t\u{0001}\"\\"}},{"A_Const":{"isnull":true,"location":-1}}]}
        })
    );
}

#[test]
fn string_escaping_matches_reference_for_every_control_byte() {
    let text: String = (0..32)
        .map(char::from)
        .chain("β<>\\\"/😀".chars())
        .collect();
    let json = node_json(&Node::String(text.clone())).unwrap();
    let escaped = serde_json::to_string(&text)
        .unwrap()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e");
    assert_eq!(json, format!(r#"{{"String":{{"sval":{escaped}}}}}"#));
}
