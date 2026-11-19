use pg19_codec::protobuf::*;
use pg19_lexer::Scanner;
use pg19_nodes::*;
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

fn repack() -> RawStmt {
    let relation = RangeVar {
        relname: Some("t".into()),
        inh: true,
        relpersistence: b'p',
        location: Location::from_pg(7).unwrap(),
        ..Default::default()
    };
    let stmt = RepackStmt {
        command: RepackCommand::REPACK_COMMAND_REPACK,
        relation: Some(Box::new(VacuumRelation {
            relation: Some(Box::new(relation)),
            ..Default::default()
        })),
        indexname: Some("i".into()),
        usingindex: true,
        ..Default::default()
    };
    RawStmt::new(stmt.into(), Location::default())
}

// [spec:pgq:req:codec.protobuf-schema/test]
#[test]
fn repack_matches_protoc_wire_fixture() {
    let statements = [repack()];
    let expected = include_bytes!("repack.pb");
    assert_eq!(statements_protobuf(&statements).unwrap(), expected);
    assert_eq!(statements_from_protobuf(expected).unwrap(), statements);
}

#[test]
#[ignore = "requires protoc on PATH"]
fn protoc_reads_native_repack_output() {
    let descriptor = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../protobuf/pg19/pg_query.pb");
    let mut child = Command::new("protoc")
        .arg("--decode=pg_query.ParseResult")
        .arg(format!("--descriptor_set_in={}", descriptor.display()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&statements_protobuf(&[repack()]).unwrap())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    for field in [
        "version: 190000",
        "repack_stmt {",
        "command: REPACK_COMMAND_REPACK",
        "relname: \"t\"",
        "indexname: \"i\"",
        "location: 7",
    ] {
        assert!(text.contains(field), "{text}");
    }
}

// [spec:pgq:req:tree.literal-variants/test]
#[test]
fn default_literals_preserve_selected_oneofs() {
    let variants = [
        Literal::Null,
        Literal::Boolean(false),
        Literal::Integer(0),
        Literal::String(String::new()),
        Literal::Float("0.000".into()),
        Literal::BitString("b0".into()),
    ];
    let mut distinct = std::collections::BTreeSet::new();
    for val in variants {
        let n = Node::AConst(AConst {
            val,
            location: Location::UNKNOWN,
        });
        let bytes = node_protobuf(&n).unwrap();
        assert!(distinct.insert(bytes.clone()));
        assert_eq!(node_from_protobuf(&bytes).unwrap(), n);
    }
}

fn qualified_setop() -> Node {
    SelectStmt {
        op: SetOperation::SETOP_UNION,
        distinct_clause: vec![Node::Null],
        larg: Some(Box::new(SelectStmt {
            target_list: vec![Node::Integer(-1), Node::String("é".into())],
            ..Default::default()
        })),
        rarg: Some(Box::new(SelectStmt {
            values_lists: vec![Node::List(vec![
                Node::IntList(vec![0, 4, -3]),
                Node::OidList(vec![u32::MAX, 9]),
            ])],
            ..Default::default()
        })),
        ..Default::default()
    }
    .into()
}

#[test]
fn nested_records_preserve_spans_and_order() {
    let mut first = repack();
    first.terminate_at(Location::from_pg(22).unwrap()).unwrap();
    let second = RawStmt::new(qualified_setop(), Location::from_pg(24).unwrap());
    let stmts = [first, second];
    assert_eq!(
        statements_from_protobuf(&statements_protobuf(&stmts).unwrap()).unwrap(),
        stmts
    );
}

// [spec:pgq:req:lex.token-results/test]
#[test]
fn scan_preserves_keywords_trivia_and_byte_spans() {
    for sql in [
        "SELECT integer join abort \"SELECT\", 'é' /* comment */",
        "  \n",
        "",
    ] {
        let tokens = Scanner::new(sql).collect::<Result<Vec<_>, _>>().unwrap();
        let frames = scan_from_protobuf(&scan_protobuf(&tokens).unwrap()).unwrap();
        assert_eq!(frames.len(), tokens.len());
        for (f, t) in frames.iter().zip(tokens) {
            assert_eq!(
                (f.start, f.end, f.kind, f.keyword),
                (t.start, t.end, t.kind, t.keyword)
            );
        }
    }
}

fn msg(number: u32, bytes: &[u8]) -> Vec<u8> {
    use prost_reflect::prost::encoding::{encode_key, encode_varint, WireType};
    let mut out = Vec::new();
    encode_key(number, WireType::LengthDelimited, &mut out);
    encode_varint(bytes.len() as u64, &mut out);
    out.extend_from_slice(bytes);
    out
}

// [spec:pgq:req:codec.errors-and-depth/test]
#[test]
fn deep_trees_and_bad_input_return_errors() {
    let mut native = Node::Boolean(false);
    let mut wire = node_protobuf(&native).unwrap();
    for _ in 0..180 {
        native = Node::List(vec![native]);
        wire = msg(270, &msg(1, &wire));
    }
    assert_eq!(node_protobuf(&native), Err(ProtobufError::DepthLimit));
    assert_eq!(node_from_protobuf(&wire), Err(ProtobufError::DepthLimit));
    assert!(matches!(
        node_from_protobuf(&[0xff]),
        Err(ProtobufError::Invalid(_))
    ));
    assert!(matches!(
        node_protobuf(&Node::XidList(vec![42])),
        Err(ProtobufError::Unsupported(_))
    ));
    assert_eq!(
        statements_from_protobuf(&[]),
        Err(ProtobufError::Version(0))
    );
    assert_eq!(
        node_from_protobuf(&node_protobuf(&qualified_setop()).unwrap()).unwrap(),
        qualified_setop()
    );
}

fn check_record<T: Default + Into<Node>>() {
    let node = T::default().into();
    let encoded = node_protobuf(&node).unwrap();
    assert_eq!(node_from_protobuf(&encoded).unwrap(), node);
}

macro_rules! node_types {
    (values { $($value:ident ($value_id:literal, $value_name:literal, $payload:ty))* }
     records { $($ty:ident ($id:literal, $pg:literal) { $($fields:tt)* })* }
    ) => {
        #[test]
        fn all_native_record_tags_round_trip() {
            $(check_record::<$ty>();)*
        }
    }
}
include!("../../pg19-nodes/src/nodes.in");

#[test]
fn concrete_pointers_obey_depth_limits() {
    let mut select = SelectStmt::default();
    for _ in 0..28 {
        select = SelectStmt {
            larg: Some(Box::new(select)),
            ..Default::default()
        };
    }
    let node = Node::from(select);
    let bytes = node_protobuf(&node).unwrap();
    assert_eq!(node_from_protobuf(&bytes).unwrap(), node);
    let mut node = Node::Integer(1);
    for _ in 0..38 {
        node = Node::List(vec![node]);
    }
    let bytes = node_protobuf(&node).unwrap();
    assert_eq!(node_from_protobuf(&bytes).unwrap(), node);
}

#[test]
fn preserves_original_parse_diagnostic() {
    use pg19_engine::{parse, Actions, ParseError, ParseMode, Reduction};
    use pg19_lexer::{Options, Token};
    use pg19_tables::Production;

    struct RejectSyntax;
    impl Actions for RejectSyntax {
        type Value = ();
        type Error = std::convert::Infallible;
        fn shift(&mut self, _: Token) {}
        fn reduce(&mut self, _: Production, _: &mut Reduction<'_, ()>) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    let rejected = parse(
        "SELECT 'é', )",
        ParseMode::Sql,
        Options::default(),
        &mut RejectSyntax,
    )
    .map(|()| Vec::<RawStmt>::new());
    let Err(SerializationError::Parse(ParseError::Diagnostic(e))) = parsed_protobuf(rejected)
    else {
        panic!("expected original parser diagnostic");
    };
    assert_eq!((e.sqlstate, e.position), ("42601", 13));
    assert_eq!(e.message, "syntax error at or near \")\"");
}

#[test]
fn special_fields_follow_the_descriptor() {
    let ddl = AlterObjectDependsStmt {
        extname: Some(String::new()),
        ..Default::default()
    };
    let table = TableFunc {
        notnulls: [0, 9, 77].into_iter().collect(),
        ..Default::default()
    };
    let stmt = CreateForeignTableStmt {
        base: CreateStmt {
            if_not_exists: true,
            ..Default::default()
        },
        servername: Some("s".into()),
        ..Default::default()
    };
    let n = Node::List(vec![ddl.into(), table.into(), stmt.into()]);
    assert_eq!(node_from_protobuf(&node_protobuf(&n).unwrap()).unwrap(), n);
    let unknown = SelectStmt {
        op: SetOperation(99),
        ..Default::default()
    };
    assert!(matches!(
        node_protobuf(&unknown.into()),
        Err(ProtobufError::Invalid(_))
    ));
}
