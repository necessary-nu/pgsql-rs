use pgsql_operations::deparse::{protobuf, statements};

#[test]
fn routine_trigger_and_rule_forms_reparse() {
    for line in include_str!("deparse-routine.jsonl").lines() {
        let [original, expected]: [String; 2] = serde_json::from_str(line).unwrap();
        let tree = pgsql_grammar::parse_statements(&original, Default::default()).unwrap();
        assert_eq!(statements(&tree).unwrap(), expected, "{original}");
        let wire = pgsql_codec::protobuf::statements_protobuf(&tree).unwrap();
        assert_eq!(protobuf(&wire).unwrap(), expected, "{original}");
    }
}
