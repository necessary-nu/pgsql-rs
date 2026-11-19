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

#[test]
fn deep_copies_preserve_typed_and_generic_descendants() {
    small_stack(|| {
        let mut lists = Node::String("terminal".into());
        for _ in 0..DEPTH {
            lists = Node::from(vec![Node::Null, lists]);
        }
        let mut select = SelectStmt {
            target_list: vec![additions(), lists],
            ..Default::default()
        };
        for _ in 0..DEPTH {
            select = SelectStmt {
                rarg: Some(select.into()),
                ..Default::default()
            };
        }
        let mut clone = select.clone();
        let mut original = &select;
        let mut copied = &mut clone;
        for _ in 0..DEPTH {
            original = original.rarg.as_deref().unwrap();
            copied = copied.rarg.as_deref_mut().unwrap();
        }
        let [sum, lists] = copied.target_list.as_mut_slice() else {
            panic!()
        };
        let mut term = sum;
        for _ in 0..DEPTH {
            let Node::AExpr(op) = term else { panic!() };
            assert_eq!(op.rexpr.as_deref(), Some(&Node::Integer(1)));
            assert_eq!(op.name.as_slice(), [Node::String("+".into())]);
            term = op.lexpr.as_deref_mut().unwrap();
        }
        assert_eq!(*term, Node::Integer(0));
        *term = Node::Integer(42);
        let mut tail = lists;
        for _ in 0..DEPTH {
            let Node::List(items) = tail else { panic!() };
            let [Node::Null, child] = items.as_mut_slice() else {
                panic!()
            };
            tail = child;
        }
        assert_eq!(*tail, Node::String("terminal".into()));
        let mut term = &original.target_list[0];
        while let Node::AExpr(expr) = term {
            term = expr.lexpr.as_deref().unwrap();
        }
        assert_eq!(*term, Node::Integer(0));
    });
}

#[test]
fn clones_embedded_records_siblings_and_nullable_values() {
    let nested = CreateForeignTableStmt {
        base: CreateStmt {
            relation: Some(Box::new(RangeVar {
                relname: Some("foo".into()),
                location: Location::from_pg(7).unwrap(),
                ..Default::default()
            })),
            table_elts: vec![
                Node::Boolean(true),
                Node::Float("00.0".into()),
                Node::IntList(vec![1, 3, -2]),
            ],
            constraints: vec![
                Node::Null,
                Node::from(vec![
                    Node::OidList(vec![1, 0]),
                    Node::BitString("x0f".into()),
                ]),
            ],
            ..Default::default()
        },
        servername: Some("mixed".into()),
        options: vec![Node::AConst(AConst {
            val: Literal::Null,
            location: Location::UNKNOWN,
        })],
    };
    let node = Node::from(nested);
    let copy = node.clone();
    assert_eq!(node, copy);
}

#[test]
fn shared_assignment_clone_and_drop_use_bounded_stacks() {
    use std::sync::Arc;
    small_stack(|| {
        let mut source = Arc::new(Node::Integer(1));
        for _ in 0..DEPTH {
            let targets = (1..=2)
                .map(|colno| {
                    Node::from(MultiAssignRef {
                        source: Some(source.clone()),
                        colno,
                        ncolumns: 2,
                    })
                })
                .collect::<Vec<_>>();
            source = Arc::new(Node::from(targets));
        }
        let duplicate = (*source).clone();
        let Node::List(original) = &*source else {
            unreachable!()
        };
        let Node::List(copy) = &duplicate else {
            unreachable!()
        };
        let (Node::MultiAssignRef(a), Node::MultiAssignRef(b)) = (&original[0], &copy[1]) else {
            unreachable!()
        };
        assert!(Arc::ptr_eq(
            a.source.as_ref().unwrap(),
            b.source.as_ref().unwrap()
        ));
        drop(source);
        drop(duplicate);
    });
}
