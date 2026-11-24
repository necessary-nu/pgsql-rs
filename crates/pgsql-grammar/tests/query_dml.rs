use pgsql_grammar::{parse_statements, Options};
use pgsql_nodes::*;

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

// [spec:pgq:sem:grammar.dml-actions/test]
#[test]
fn insert_preserves_source_order_conflict_action_and_returning() {
    let sql = "INSERT INTO t(id) VALUES (2),(3) ON CONFLICT(id) DO SELECT RETURNING *";
    let statements = parse_statements(sql, Options::default()).unwrap();
    let Some(Node::InsertStmt(insert)) = statements[0].stmt.as_deref() else {
        panic!("expected INSERT")
    };
    assert_eq!(
        insert.relation.as_ref().unwrap().relname.as_deref(),
        Some("t")
    );
    let [Node::ResTarget(id)] = insert.cols.as_slice() else {
        panic!("expected INSERT column")
    };
    assert_eq!(id.name.as_deref(), Some("id"));
    let Some(Node::SelectStmt(source)) = insert.select_stmt.as_deref() else {
        panic!("expected VALUES")
    };
    let values = source
        .values_lists
        .iter()
        .map(|row| {
            let Node::List(row) = row else {
                panic!("expected VALUES row")
            };
            let [Node::AConst(AConst {
                val: Literal::Integer(i),
                ..
            })] = row.as_slice()
            else {
                panic!("expected one integer")
            };
            *i
        })
        .collect::<Vec<_>>();
    assert_eq!(values, [2, 3]);
    let conflict = insert.on_conflict_clause.as_ref().unwrap();
    assert_eq!(conflict.action, OnConflictAction::ONCONFLICT_SELECT);
    assert!(conflict.infer.is_some());
    let [Node::ResTarget(target)] = insert.returning_clause.as_ref().unwrap().exprs.as_slice()
    else {
        panic!("expected RETURNING target")
    };
    assert!(
        matches!(target.val.as_deref(),Some(Node::ColumnRef(column)) if matches!(column.fields.as_slice(),[Node::AStar(_)]))
    );
}

// [spec:pgq:sem:grammar.dml-actions/test]
#[test]
fn merge_keeps_conditional_actions_in_statement_order() {
    let sql="MERGE INTO t USING s ON t.i = s.i WHEN MATCHED AND s.i > 1 THEN DELETE WHEN NOT MATCHED THEN INSERT (i) VALUES(s.i) RETURNING t.i";
    let statements = parse_statements(sql, Options::default()).unwrap();
    let Some(Node::MergeStmt(merge)) = statements[0].stmt.as_deref() else {
        panic!("expected MERGE")
    };
    let [Node::MergeWhenClause(delete), Node::MergeWhenClause(insert)] =
        merge.merge_when_clauses.as_slice()
    else {
        panic!("expected two merge actions")
    };
    assert_eq!(delete.command_type, CmdType::CMD_DELETE);
    assert_eq!(insert.command_type, CmdType::CMD_INSERT);
    assert!(delete.condition.is_some());
    assert!(insert.condition.is_none());
    assert_eq!(insert.values.len(), 1);
    assert_eq!(merge.returning_clause.as_ref().unwrap().exprs.len(), 1);
}
