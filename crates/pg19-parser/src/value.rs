use pg19_nodes::{Node, NodeBox, SelectStmt, StatementList};

#[derive(Default)]
pub(crate) enum Value {
    #[default]
    Empty,
    Limit(Box<crate::limits::SelectLimit>),
    Integer(i32),
    Text(String),
    Node(Node),
    List(Vec<Node>),
    Statements(StatementList),
    Quantifier(Quantifier),
    Group(Vec<Node>, bool),
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Quantifier {
    Default,
    All,
    Distinct,
}

pub(crate) type Checked<T> = Result<T, &'static str>;

impl Value {
    pub(crate) fn integer(self) -> Checked<i32> {
        match self {
            Self::Integer(value) => Ok(value),
            _ => Err("expected integer"),
        }
    }
    pub(crate) fn text(self) -> Checked<String> {
        match self {
            Self::Text(value) => Ok(value),
            _ => Err("expected text"),
        }
    }
    pub(crate) fn node(self) -> Checked<Node> {
        match self {
            Self::Node(value) => Ok(value),
            _ => Err("expected node"),
        }
    }
    pub(crate) fn optional(self) -> Checked<Option<Box<Node>>> {
        match self {
            Self::Empty => Ok(None),
            Self::Node(value) => Ok(Some(Box::new(value))),
            _ => Err("expected nullable node"),
        }
    }
    pub(crate) fn list(self) -> Checked<Vec<Node>> {
        match self {
            Self::List(value) => Ok(value),
            _ => Err("expected list"),
        }
    }
    pub(crate) fn select(self) -> Checked<NodeBox<SelectStmt>> {
        match self {
            Self::Node(Node::SelectStmt(value)) => Ok(value),
            _ => Err("expected SelectStmt"),
        }
    }
    pub(crate) fn statements(self) -> Checked<StatementList> {
        match self {
            Self::Statements(value) => Ok(value),
            _ => Err("expected statements"),
        }
    }
    pub(crate) fn quantifier(self) -> Checked<Quantifier> {
        match self {
            Self::Quantifier(value) => Ok(value),
            _ => Err("expected set quantifier"),
        }
    }
    pub(crate) fn group(self) -> Checked<(Vec<Node>, bool)> {
        match self {
            Self::Group(values, distinct) => Ok((values, distinct)),
            _ => Err("expected GROUP BY"),
        }
    }
}
