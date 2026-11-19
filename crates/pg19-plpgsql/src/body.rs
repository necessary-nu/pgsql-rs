use crate::{Error, ParseMode};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataType {
    Scalar(String),
    Record,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expression {
    pub query: String,
    pub mode: ParseMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variable {
    pub name: String,
    pub lineno: usize,
    pub datatype: DataType,
    pub constant: bool,
    pub not_null: bool,
    pub default: Option<Expression>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Datum {
    Variable(Variable),
    RecordField { name: String, parent: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub lineno: usize,
    pub label: Option<String>,
    pub body: Vec<Statement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Statement {
    Block(Block),
    Assign {
        lineno: usize,
        datum: usize,
        expression: Expression,
    },
    Perform {
        lineno: usize,
        expression: Expression,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledBody {
    pub datums: Vec<Datum>,
    pub block: Block,
}

pub trait EmbeddedSql {
    /// The SQL parser and type catalog report positions relative to the supplied text.
    fn datatype(&mut self, sql: &str) -> Result<DataType, Error>;
    fn validate(&mut self, sql: &str, mode: ParseMode) -> Result<(), Error>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum CompileError {
    Diagnostic(Error),
    Unsupported { production: &'static str },
}

impl From<Error> for CompileError {
    fn from(e: Error) -> Self {
        Self::Diagnostic(e)
    }
}
