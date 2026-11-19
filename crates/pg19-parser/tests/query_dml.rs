use pg19_nodes::*;
use pg19_parser::{parse_statements, Options};

// [spec:pgq:sem:grammar.query-actions/test]
#[test]
fn select_preserves_targets_joins_and_set_precedence() {
    let sql="SELECT l.a AS first, r.b AS second FROM l LEFT JOIN r ON l.id = r.id UNION SELECT 3,4 INTERSECT SELECT 5,6";
    let statements = parse_statements(sql, Options::default()).unwrap();
    let Some(Node::SelectStmt(union)) = statements[0].stmt.as_deref() else {
        panic!("expected SELECT")
    };
    assert_eq!(union.op, SetOperation::SETOP_UNION);
    assert_eq!(
        union.rarg.as_deref().unwrap().op,
        SetOperation::SETOP_INTERSECT
    );
    let left = union.larg.as_deref().unwrap();
    let aliases: Vec<_> = left
        .target_list
        .iter()
        .map(|node| {
            let Node::ResTarget(t) = node else {
                panic!("expected result target")
            };
            t.name.as_deref().unwrap()
        })
        .collect();
    assert_eq!(aliases, ["first", "second"]);
    let [Node::JoinExpr(join)] = left.from_clause.as_slice() else {
        panic!("expected JOIN")
    };
    assert_eq!(join.jointype, JoinType::JOIN_LEFT);
    let relname = |arg: &Option<Box<Node>>| {
        let Some(Node::RangeVar(r)) = arg.as_deref() else {
            panic!("expected relation")
        };
        r.relname.as_deref().unwrap().to_owned()
    };
    assert_eq!(relname(&join.larg), "l");
    assert_eq!(relname(&join.rarg), "r");
    assert!(matches!(join.quals.as_deref(),Some(Node::AExpr(a)) if a.kind==AExprKind::AEXPR_OP));
}
