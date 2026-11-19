use pg19_nodes::*;
use pg19_operations::deparse::{comments, protobuf, statements, Comment, Error};

// [spec:pgq:req:deparse.stream/test]
#[test]
fn render_the_pg19_reference_statements_as_one_stream() {
    let wire = include_bytes!("deparse.pb");
    let expected = include_str!("deparse.expected.sql").trim_end_matches('\n');
    let stmts = pg19_codec::protobuf::statements_from_protobuf(wire).unwrap();
    assert_eq!(statements(&[]).unwrap(), "");
    assert_eq!(statements(&stmts).unwrap(), expected);
    assert_eq!(protobuf(wire).unwrap(), expected);
    assert!(!expected.ends_with(';'));
}

#[test]
fn missing_children_and_invalid_wire_are_recoverable() {
    assert!(matches!(protobuf(&[0x80]), Err(Error::Protobuf(_))));
    let incomplete = RawStmt::new(WaitStmt::default().into(), Location::UNKNOWN);
    let wire = pg19_codec::protobuf::statements_protobuf(&[incomplete]).unwrap();
    assert_eq!(
        protobuf(&wire),
        Err(Error::InvalidField("WaitStmt.lsn_literal"))
    );
    let stmt = RawStmt::new(
        NotifyStmt {
            conditionname: Some("value".into()),
            payload: Some(String::new()),
            ..Default::default()
        }
        .into(),
        Location::UNKNOWN,
    );
    assert_eq!(statements(&[stmt]).unwrap(), "NOTIFY value, ''");
    assert!(protobuf(include_bytes!("deparse.pb")).is_ok());
}

#[test]
fn comments_keep_delimiters_and_assign_intervening_lines_once() {
    let sql = "\n-- one\r\n\n/* outer /* nested */ */\nLISTEN \"é\"; \n-- two\nNOTIFY \"--literal\"; -- end\n\n";
    let first_end = sql.find(';').unwrap() + 1;
    let second_end = sql.rfind(';').unwrap() + 1;
    assert_eq!(
        comments(sql).unwrap(),
        [
            Comment {
                text: "-- one",
                match_location: 0,
                newlines_before: 1,
                newlines_after: 2
            },
            Comment {
                text: "/* outer /* nested */ */",
                match_location: 0,
                newlines_before: 0,
                newlines_after: 1
            },
            Comment {
                text: "-- two",
                match_location: first_end,
                newlines_before: 1,
                newlines_after: 1
            },
            Comment {
                text: "-- end",
                match_location: second_end,
                newlines_before: 0,
                newlines_after: 0
            },
        ]
    );
    assert!(comments("SELECT 1; -- first\n/* unclosed").is_err());
    assert!(comments("NOTIFY x, '--data';").unwrap().is_empty());
}

#[test]
fn compact_queries_and_dml_preserve_the_reference_spelling() {
    for line in include_str!("deparse-query.jsonl")
        .lines()
        .chain(include_str!("deparse-dml.jsonl").lines())
        .chain(include_str!("deparse-fn.jsonl").lines())
    {
        let [original, expected]: [String; 2] = serde_json::from_str(line).unwrap();
        let tree = pg19_parser::parse_statements(&original, Default::default()).unwrap();
        assert_eq!(statements(&tree).unwrap(), expected, "{original}");
        let wire = pg19_codec::protobuf::statements_protobuf(&tree).unwrap();
        assert_eq!(protobuf(&wire).unwrap(), expected, "{original}");
    }
}

#[test]
fn deep_left_associative_arithmetic_uses_a_worklist() {
    std::thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(|| {
            let sql = format!("SELECT 1{}", " + 1".repeat(10000));
            let tree = pg19_parser::parse_statements(&sql, Default::default()).unwrap();
            assert_eq!(statements(&tree).unwrap(), sql);
        })
        .unwrap()
        .join()
        .unwrap();
}

// [spec:pgq:req:deparse.pretty/test]
// [spec:pgq:req:deparse.comments/test]
// [spec:pgq:req:deparse.protobuf/test]
#[test]
fn native_and_wire_formats_preserve_comments() {
    use pg19_operations::deparse::{protobuf_with_options, statements_with_options, Options};
    let fixtures: Vec<([usize; 5], String, String)> =
        serde_json::from_str(include_str!("deparse-format.json")).unwrap();
    for ([pretty, indent, width, first, newline], sql, expected) in fixtures {
        let options = Options {
            pretty_print: pretty != 0,
            indent_size: indent,
            max_line_length: width,
            commas_start_of_line: first != 0,
            trailing_newline: newline != 0,
        };
        let comments = comments(&sql).unwrap();
        let raw = pg19_parser::parse_statements(&sql, Default::default()).unwrap();
        assert_eq!(
            statements_with_options(&raw, options, &comments).unwrap(),
            expected,
            "{sql}"
        );
        let wire = pg19_codec::protobuf::statements_protobuf(&raw).unwrap();
        assert_eq!(
            protobuf_with_options(&wire, options, &comments).unwrap(),
            expected,
            "{sql}"
        );
        assert_eq!(
            pg19_parser::parse_statements(&expected, Default::default())
                .unwrap()
                .len(),
            raw.len(),
            "{sql}"
        );
    }
    assert!(matches!(
        protobuf_with_options(&[0x80], Options::default(), &[]),
        Err(Error::Protobuf(_))
    ));
}
