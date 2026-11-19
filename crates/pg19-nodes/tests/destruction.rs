use pg19_nodes::*;

const DEPTH: usize = 20_000;

fn small_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

fn additions() -> Node {
    let mut node = Node::Integer(0);
    for _ in 0..DEPTH {
        node = AExpr {
            name: vec![Node::String("+".into())],
            lexpr: Some(Box::new(node)),
            rexpr: Some(Box::new(Node::Integer(1))),
            ..Default::default()
        }
        .into();
    }
    node
}

#[test]
fn drops_external_generic_nodes_and_nested_lists() {
    small_stack(|| {
        let mut n = additions();
        for _ in 0..DEPTH {
            n = Node::from(vec![Node::Null, n]);
        }
        drop(RawStmt::new(n, Location::default()));
    });
}

#[test]
fn drops_bare_typed_records_after_partial_moves() {
    small_stack(|| {
        let mut select = SelectStmt {
            target_list: vec![additions()],
            ..Default::default()
        };
        for _ in 0..DEPTH {
            select = SelectStmt {
                rarg: Some(select.into()),
                ..Default::default()
            };
        }
        let root = Node::from(select);
        let Node::SelectStmt(owner) = root else {
            unreachable!()
        };
        let SelectStmt { rarg, .. } = *owner.into_box();
        let mut record = rarg.unwrap().into_inner();
        record.rarg = None;
        assert!(record.rarg.is_none());
    });
}

#[test]
fn list_iteration_and_owner_access_preserve_values() {
    let mut list: NodeList = [Node::Integer(1)].into_iter().collect();
    list.extend([Node::Integer(2)]);
    for x in &mut list {
        if let Node::Integer(v) = x {
            *v += 1;
        }
    }
    assert_eq!(list.into_vec(), [Node::Integer(2), Node::Integer(3)]);
    let value = Box::new(NotifyStmt {
        conditionname: Some("release".into()),
        ..Default::default()
    });
    let address: *const NotifyStmt = &*value;
    let Node::NotifyStmt(owner) = Node::from(value) else {
        unreachable!()
    };
    assert_eq!(address, owner.as_ref() as *const NotifyStmt);
    let NotifyStmt { conditionname, .. } = owner.into_inner();
    assert_eq!(conditionname.as_deref(), Some("release"));
}
