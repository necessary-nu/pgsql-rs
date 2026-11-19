use pg19_nodes::{
    AConst, DefElem, Literal, Location, Node, NodeTag, RawStmt, RepackCommand, ResTarget,
    SelectStmt, StatementList, StatementSpanError,
};

const COMMENTS_SQL: &str = include_str!("../../../tools/pg19/fixtures/comments.sql");
const COMMENTS: &str = include_str!("../../../tools/pg19/fixtures/comments.patched.json");
const REPACK: &str = include_str!("../../../tools/pg19/fixtures/repack.patched.json");
const WAIT: &str = include_str!("../../../tools/pg19/fixtures/wait.patched.json");

fn loc(byte: usize) -> Location {
    Location::from_byte_offset(byte).unwrap()
}

// All fixture fields inspected here contain scalar tokens, quoted simple strings,
// child records, or lists. Skip children to read each record's own field order.
fn fixture_field_names(snapshot: &str, tag: NodeTag) -> Vec<&str> {
    let marker = format!("{{{} ", tag.pg_name().to_uppercase());
    let body = snapshot.split_once(&marker).unwrap().1.as_bytes();
    let mut names = Vec::new();
    let mut depth = 0;
    for (at, &byte) in body.iter().enumerate() {
        match byte {
            b'}' if depth == 0 => break,
            b'{' | b'(' => depth += 1,
            b'}' | b')' => depth -= 1,
            b':' if depth == 0 => {
                let tail = &body[at + 1..];
                let end = tail.iter().position(|byte| *byte == b' ').unwrap();
                names.push(std::str::from_utf8(&tail[..end]).unwrap());
            }
            _ => {}
        }
    }
    names
}

// [spec:pgq:req:tree.node-layout/test]
#[test]
fn record_layout_matches_patched_pg19() {
    for (snapshot, tag) in [
        (COMMENTS, NodeTag::RawStmt),
        (COMMENTS, NodeTag::SelectStmt),
        (COMMENTS, NodeTag::ResTarget),
        (REPACK, NodeTag::RepackStmt),
        (REPACK, NodeTag::VacuumRelation),
        (REPACK, NodeTag::RangeVar),
        (REPACK, NodeTag::DefElem),
        (WAIT, NodeTag::WaitStmt),
    ] {
        assert_eq!(
            tag.record_fields().unwrap(),
            fixture_field_names(snapshot, tag)
        );
    }
    assert_eq!(RepackCommand::REPACK_COMMAND_REPACK.0, 2);
    assert!(REPACK.contains("{REPACKSTMT :command 2 "));
}

fn select_literal(val: Literal, location: Location) -> SelectStmt {
    let target = ResTarget {
        val: Some(Box::new(AConst { val, location }.into())),
        location,
        ..Default::default()
    };
    SelectStmt {
        target_list: vec![target.into()],
        ..Default::default()
    }
}

// [spec:pgq:req:tree.node-locations/test]
#[test]
fn nested_locations_preserve_bytes_and_unknown() {
    let byte = COMMENTS_SQL.rfind('2').unwrap();
    assert_ne!(byte, COMMENTS_SQL[..byte].chars().count());
    let stmt = select_literal(Literal::Integer(2), loc(byte));
    let [Node::ResTarget(target)] = stmt.target_list.as_slice() else {
        panic!("expected target")
    };
    let Some(Node::AConst(constant)) = target.val.as_deref() else {
        panic!("expected constant")
    };
    let Literal::Integer(value) = constant.val else {
        panic!("expected integer")
    };
    let literal = format!(
        "{{A_CONST :val {value} :location {}}}",
        constant.location.as_pg()
    );
    assert!(COMMENTS.contains(&format!(
        ":val {literal} :location {}}}",
        target.location.as_pg()
    )));

    let def = DefElem {
        arg_location: Location::UNKNOWN,
        ..Default::default()
    };
    assert!(REPACK.contains(&format!(":arg_location {}}}", def.arg_location.as_pg())));
    assert_eq!(def.arg_location.byte_offset(), None);
    assert_eq!(Location::from_pg(0).unwrap().byte_offset(), Some(0));
    assert_eq!(Location::from_pg(-2), None);
    assert_eq!(Location::from_byte_offset(i32::MAX as usize + 1), None);
}

#[test]
fn distinguishes_empty_and_null_list_entries_and_constants() {
    let mut select = SelectStmt::default();
    assert!(select.distinct_clause.is_empty());
    select.distinct_clause.push(Node::Null);
    assert_eq!(select.distinct_clause[0].tag(), None);
    select.values_lists = vec![Node::List(vec![Node::Integer(0), Node::Boolean(false)])];
    assert_eq!(select.values_lists[0].tag(), Some(NodeTag::List));
    assert_ne!(Literal::Null, Literal::String(String::new()));
    assert_ne!(Literal::Integer(0), Literal::Boolean(false));
}

// [spec:pgq:req:tree.statement-spans/test]
#[test]
fn statement_spans_handle_empty_statements_and_eof() {
    let mut statements = StatementList::default();
    let starts: Vec<_> = COMMENTS_SQL
        .match_indices("SELECT")
        .map(|(i, _)| i)
        .collect();
    for (start, (end, _)) in starts.into_iter().zip(COMMENTS_SQL.match_indices(';')) {
        statements.push(Some(SelectStmt::default().into()), loc(start));
        statements.terminate_last_at(loc(end)).unwrap();
    }
    for stmt in statements.as_slice() {
        let range = stmt.source_range(COMMENTS_SQL.len()).unwrap();
        let metadata = format!(
            ":stmt_location {} :stmt_len {}",
            stmt.stmt_location.as_pg(),
            stmt.stmt_len
        );
        assert!(COMMENTS.contains(&metadata));
        assert!(COMMENTS_SQL[range].starts_with("SELECT"));
    }

    let mut list = StatementList::default();
    list.push(None, Location::UNKNOWN);
    list.terminate_last_at(loc(0)).unwrap();
    list.push(
        Some(select_literal(Literal::Integer(1), loc(7)).into()),
        loc(0),
    );
    list.terminate_last_at(loc(8)).unwrap();
    list.push(None, Location::UNKNOWN);
    list.terminate_last_at(loc(9)).unwrap();
    list.push(
        Some(select_literal(Literal::Integer(2), loc(27)).into()),
        loc(20),
    );
    let sql = "SELECT 1;; /* é */ SELECT 2";
    let [first, second] = list.as_slice() else {
        panic!("expected two statements")
    };
    assert_eq!(first.stmt_len, 8);
    assert_eq!(second.stmt_len, 0);
    assert_eq!(first.source_range(sql.len()), Some(0..8));
    assert_eq!(&sql[second.source_range(sql.len()).unwrap()], "SELECT 2");
}

#[test]
fn invalid_source_span_is_detectable() {
    let mut stmt = RawStmt::new(SelectStmt::default().into(), loc(3));
    assert_eq!(
        stmt.terminate_at(loc(2)),
        Err(StatementSpanError::EndBeforeStart)
    );
    assert!(stmt.source_range(2).is_none());
    assert_eq!(
        stmt.terminate_at(Location::UNKNOWN),
        Err(StatementSpanError::UnknownOffset)
    );
    stmt.stmt_len = -1;
    assert!(stmt.source_range(10).is_none());
}
