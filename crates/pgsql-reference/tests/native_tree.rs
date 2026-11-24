use pgsql_nodes::*;
use pgsql_reference::{node_text, nodes_text, statements_text, TreeError};
use serde_json::Value;

fn loc(n: i32) -> Location {
    Location::from_pg(n).unwrap()
}

fn constant(value: Literal, offset: i32) -> Node {
    AConst {
        val: value,
        location: loc(offset),
    }
    .into()
}

fn select_targets(targets: impl IntoIterator<Item = (Node, i32)>) -> Node {
    let target_list = targets
        .into_iter()
        .map(|(value, offset)| {
            ResTarget {
                val: Some(Box::new(value)),
                location: loc(offset),
                ..Default::default()
            }
            .into()
        })
        .collect();
    SelectStmt {
        target_list,
        ..Default::default()
    }
    .into()
}

fn boolean_expression() -> Node {
    let any = AExpr {
        kind: AExprKind::AEXPR_OP_ANY,
        name: vec![Node::String("=".into())],
        lexpr: Some(Box::new(constant(Literal::Integer(1), 7))),
        rexpr: Some(Box::new(
            ColumnRef {
                fields: vec![Node::String("arr".into())],
                location: loc(16),
            }
            .into(),
        )),
        location: loc(9),
        ..Default::default()
    };
    let not = BoolExpr {
        boolop: BoolExprType::NOT_EXPR,
        args: vec![constant(Literal::Boolean(false), 28)],
        location: loc(24),
    };
    let disjunction = BoolExpr {
        boolop: BoolExprType::OR_EXPR,
        args: vec![any.into(), not.into()],
        location: loc(21),
    };
    select_targets([
        (disjunction.into(), 7),
        (constant(Literal::Null, 35), 35),
        (constant(Literal::BitString("b10".into()), 41), 41),
        (constant(Literal::Float("60.50".into()), 48), 48),
    ])
}

fn foreign_table() -> Node {
    let relation = RangeVar {
        relname: Some("9 t".into()),
        inh: true,
        relpersistence: b'p',
        location: loc(21),
        ..Default::default()
    };
    CreateForeignTableStmt {
        base: CreateStmt {
            relation: Some(Box::new(relation)),
            ..Default::default()
        },
        servername: Some("s{x".into()),
        ..Default::default()
    }
    .into()
}

fn string_values() -> Node {
    let fields = [
        (Literal::String("x (a)".into()), 7),
        (Literal::String("\n".into()), 16),
        (Literal::String("é".into()), 23),
        (Literal::BitString("b1".into()), 31),
        (Literal::Boolean(true), 37),
        (Literal::Float("1.2E+5".into()), 43),
        (Literal::String(String::new()), 51),
    ];
    select_targets(fields.into_iter().map(|(v, at)| (constant(v, at), at)))
}

#[test]
fn typed_raw_trees_match_pg19_bytes() {
    let snapshots = include_str!("tree-output.jsonl")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let nodes = [boolean_expression(), foreign_table(), string_values()];
    assert_eq!(nodes.len(), snapshots.len());
    for (stmt, expected) in nodes.into_iter().zip(snapshots) {
        let raw = RawStmt::new(stmt, loc(0));
        assert_eq!(
            statements_text(std::slice::from_ref(&raw)).unwrap(),
            expected["tree"],
            "{}",
            expected["sql"]
        );
        assert_eq!(nodes_text(&[raw.into()]).unwrap(), expected["tree"]);
    }
}

#[test]
fn empty_lists_scalar_lists_and_token_escaping() {
    assert_eq!(nodes_text(&[]).unwrap(), "<>");
    let node = Node::from(vec![
        Node::Null,
        Node::Integer(-3),
        Node::String("-0".into()),
        Node::String("+.".into()),
        Node::String("a\t(b)\\{c}".into()),
        Node::String("<>".into()),
        Node::String("\"a\"b".into()),
        Node::Float("-1.20e-05".into()),
        Node::BitString("xAb".into()),
        Node::Boolean(false),
        Node::IntList(vec![1, -3]),
        Node::OidList(vec![u32::MAX]),
        Node::XidList(vec![12]),
    ]);
    assert_eq!(node_text(&node).unwrap(), "(<> -3 \"\\-0\" \"\\+.\" \"a\\\t\\(b\\)\\\\\\{c\\}\" \"\\<>\" \"\\\"a\"b\" -1.20e-05 xAb false (i 1 -3) (o 4294967295) (x 12))");
}

#[test]
fn custom_rte_writers_select_their_fields() {
    let entry = RangeTblEntry {
        rtekind: RTEKind::RTE_NAMEDTUPLESTORE,
        enrtuples: 2.5e-5,
        enrname: Some("r".into()),
        ..Default::default()
    };
    let text = node_text(&entry.into()).unwrap();
    assert!(text.contains(" :enrname r :enrtuples 2.5e-05 :coltypes <>"));
    assert!(!text.contains(" :relkind"));
    let entry = RangeTblEntry {
        rtekind: RTEKind(-20),
        ..Default::default()
    };
    assert_eq!(
        node_text(&entry.into()),
        Err(TreeError::UnknownEnum("RTEKind", -20))
    );
}

#[test]
fn deep_lists_use_the_explicit_stack() {
    std::thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(|| {
            let mut n = Node::Integer(1);
            for _ in 0..2000 {
                n = Node::from(vec![n]);
            }
            let tree = node_text(&n).unwrap();
            assert_eq!(tree, format!("{}1{}", "(".repeat(2000), ")".repeat(2000)));
            while let Node::List(mut list) = n {
                n = list.pop().unwrap();
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
#[ignore = "set PG19_REFERENCE_DIR to a completed reference build"]
fn verify_output_snapshots_with_patched_postgres() {
    let probe = std::path::PathBuf::from(std::env::var("PG19_REFERENCE_DIR").unwrap())
        .join("patched/probe");
    for row in include_str!("tree-output.jsonl").lines() {
        let wanted: Value = serde_json::from_str(row).unwrap();
        let result = std::process::Command::new(&probe)
            .arg(wanted["sql"].as_str().unwrap())
            .output()
            .unwrap();
        assert!(result.status.success());
        let got: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(got["tree"], wanted["tree"]);
    }
}
