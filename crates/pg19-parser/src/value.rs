use pg19_nodes::{Node, NodeBox, SelectStmt, StatementList};

#[derive(Default)]
pub(crate) enum Value {
    #[default]
    Empty,
    Integer(i32),
    Keyword(&'static str),
    Quantifier(Quantifier),
    // The parser discards punctuation and moved values on almost every reduction.
    // Only this variant needs to run drop glue for a semantic payload.
    Owned(OwnedValue),
}

pub(crate) enum OwnedValue {
    Limit(Box<crate::limits::SelectLimit>),
    Text(String),
    Node(Node),
    List(Vec<Node>),
    Statements(StatementList),
    KeyAction(u8, Vec<Node>),
    KeyActions(u8, u8, Vec<Node>),
    TriggerEvents(i16, Vec<Node>),
    PrivilegeTarget(crate::utility::access::PrivilegeTarget),
    Group(Vec<Node>, bool),
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Quantifier {
    Default,
    All,
    Distinct,
}

pub(crate) type Checked<T> = Result<T, &'static str>;

#[cold]
#[inline(never)]
fn unexpected(value: Value, expected: &'static str) -> &'static str {
    drop(value);
    expected
}

impl Value {
    #[inline]
    pub(crate) fn from_node(node: Node) -> Self {
        Self::Owned(OwnedValue::Node(node))
    }
    #[inline]
    pub(crate) fn from_list(list: Vec<Node>) -> Self {
        Self::Owned(OwnedValue::List(list))
    }
    #[inline]
    pub(crate) fn from_text(text: String) -> Self {
        Self::Owned(OwnedValue::Text(text))
    }

    pub(crate) fn cell(self) -> Checked<Node> {
        match self {
            Self::Empty => Ok(Node::Null),
            Self::Owned(OwnedValue::Node(node)) => Ok(node),
            Self::Owned(OwnedValue::List(list)) if list.is_empty() => Ok(Node::Null),
            Self::Owned(OwnedValue::List(list)) => Ok(Node::from(list)),
            other => Err(unexpected(other, "expected a nullable list element")),
        }
    }
    pub(crate) fn integer(self) -> Checked<i32> {
        match self {
            Self::Integer(value) => Ok(value),
            other => Err(unexpected(other, "expected integer")),
        }
    }
    pub(crate) fn text(self) -> Checked<String> {
        match self {
            Self::Owned(OwnedValue::Text(value)) => Ok(value),
            Self::Keyword(word) => Ok(word.to_owned()),
            other => Err(unexpected(other, "expected text")),
        }
    }
    pub(crate) fn node(self) -> Checked<Node> {
        match self {
            Self::Owned(OwnedValue::Node(value)) => Ok(value),
            other => Err(unexpected(other, "expected node")),
        }
    }
    pub(crate) fn optional(self) -> Checked<Option<Box<Node>>> {
        match self {
            Self::Empty => Ok(None),
            Self::Owned(OwnedValue::Node(value)) => Ok(Some(Box::new(value))),
            other => Err(unexpected(other, "expected nullable node")),
        }
    }
    pub(crate) fn indirection(self) -> Checked<Vec<Node>> {
        match self {
            Self::Owned(OwnedValue::Node(first)) => Ok(vec![first]),
            Self::Owned(OwnedValue::List(fields)) => Ok(fields),
            other => Err(unexpected(other, "expected indirection")),
        }
    }
    pub(crate) fn list(self) -> Checked<Vec<Node>> {
        match self {
            Self::Owned(OwnedValue::List(value)) => Ok(value),
            other => Err(unexpected(other, "expected list")),
        }
    }
    pub(crate) fn select(self) -> Checked<NodeBox<SelectStmt>> {
        match self {
            Self::Owned(OwnedValue::Node(Node::SelectStmt(value))) => Ok(value),
            other => Err(unexpected(other, "expected SelectStmt")),
        }
    }
    pub(crate) fn statements(self) -> Checked<StatementList> {
        match self {
            Self::Owned(OwnedValue::Statements(value)) => Ok(value),
            other => Err(unexpected(other, "expected statements")),
        }
    }
    pub(crate) fn quantifier(self) -> Checked<Quantifier> {
        match self {
            Self::Quantifier(value) => Ok(value),
            other => Err(unexpected(other, "expected set quantifier")),
        }
    }
    pub(crate) fn group(self) -> Checked<(Vec<Node>, bool)> {
        match self {
            Self::Owned(OwnedValue::Group(values, distinct)) => Ok((values, distinct)),
            other => Err(unexpected(other, "expected GROUP BY")),
        }
    }
}
