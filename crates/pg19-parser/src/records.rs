//! Shared constructors used by the PostgreSQL semantic action translator.
use super::{value::Value, Failure};
use pg19_engine::Reduction;
use pg19_nodes::*;
macro_rules! record {
    ($value:expr, $record:ident) => {
        match $value {
            Value::Empty => None,
            Value::Node(Node::$record(v)) => Some(v.into_box()),
            _ => return Err(concat!("expected ", stringify!($record)).into()),
        }
    };
}
pub(crate) use record;
pub(super) fn optional_node(value: Value) -> Result<Option<Box<Node>>, Failure> {
    Ok(match value.cell()? {
        Node::Null => None,
        node => Some(Box::new(node)),
    })
}

pub(crate) fn optional_text(value: Value) -> Result<Option<String>, Failure> {
    Ok(match value {
        Value::Empty => None,
        Value::Text(text) => Some(text),
        _ => return Err("expected nullable text".into()),
    })
}

fn persistence(value: Value, storage: i32) -> Result<Value, Failure> {
    let Node::RangeVar(mut range) = value.node()? else {
        return Err("expected RangeVar".into());
    };
    range.relpersistence = storage as u8;
    Ok(Value::Node(Node::RangeVar(range)))
}

type ColumnQualifiers = (Vec<Node>, Option<Box<CollateClause>>);

fn column_qualifiers(items: Vec<Node>) -> Result<ColumnQualifiers, Failure> {
    let mut constraints = Vec::new();
    let mut collation = None;
    for item in items {
        match item {
            Node::Constraint(_) => constraints.push(item),
            Node::CollateClause(clause) => {
                if collation.is_some() {
                    return Err(Failure::At(
                        "multiple COLLATE clauses not allowed".to_owned(),
                        "42601",
                        clause.location,
                    ));
                }
                collation = Some(clause.into_box());
            }
            _ => return Err("expected a column constraint or collation".into()),
        }
    }
    Ok((constraints, collation))
}

fn dotted_name(nodes: Vec<Node>) -> Result<String, Failure> {
    let mut names = Vec::with_capacity(nodes.len());
    for node in nodes {
        match node {
            Node::String(name) => names.push(name),
            Node::AStar(_) => names.push("*".to_owned()),
            _ => return Err("expected dotted identifier".into()),
        }
    }
    Ok(names.join("."))
}

fn numeric_literal(node: Node) -> Result<Literal, Failure> {
    Ok(match node {
        Node::Integer(n) => Literal::Integer(n),
        Node::Float(s) => Literal::Float(s),
        _ => return Err("expected numeric literal".into()),
    })
}

fn with_arg_location(value: Value, location: Location) -> Result<Value, Failure> {
    let Node::DefElem(mut option) = value.node()? else {
        return Err("expected DefElem".into());
    };
    option.arg_location = location;
    Ok(Value::Node(Node::DefElem(option)))
}

fn append(mut list: Vec<Node>, item: Node) -> Vec<Node> {
    list.push(item);
    list
}

fn prepend(item: Node, mut list: Vec<Node>) -> Vec<Node> {
    list.insert(0, item);
    list
}

type Action = fn(&mut Reduction<'_, Value>) -> Result<Value, Failure>;
include!("records.in");
