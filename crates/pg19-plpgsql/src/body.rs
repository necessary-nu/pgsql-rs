use crate::{Error, ParseMode};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataType {
    Scalar(String),
    Record,
    Pseudo(String),
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
    Promise { variable: Variable, promise: u8 },
    Row { fields: Vec<(String, usize)> },
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
    Return {
        lineno: usize,
        datum: Option<usize>,
        expression: Option<Expression>,
    },
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
    fn datatype(&mut self, sql: &str) -> Result<DataType, CompileError>;
    fn validate(&mut self, sql: &str, mode: ParseMode) -> Result<(), CompileError>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum CompileError {
    Diagnostic(Error),
    SqlAction(pg19_parser::Error),
    InvalidTree(&'static str),
    Unsupported { production: &'static str },
}

impl From<Error> for CompileError {
    fn from(e: Error) -> Self {
        Self::Diagnostic(e)
    }
}

impl CompileError {
    pub(crate) fn in_body(self, source: &str, start: usize) -> Self {
        match self {
            Self::Diagnostic(mut e) => {
                e.position = source[..start].chars().count() + e.position.max(1);
                Self::Diagnostic(e)
            }
            other => other,
        }
    }
}

impl From<pg19_parser::Error> for CompileError {
    fn from(e: pg19_parser::Error) -> Self {
        match e {
            pg19_parser::Error::Diagnostic(diagnostic) => Self::Diagnostic(diagnostic),
            other => Self::SqlAction(other),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Returns {
    Value,
    Void { procedure: bool },
    Out(usize),
    Set { out: Option<usize> },
}

impl Variable {
    pub fn implicit(name: impl Into<String>, datatype: DataType) -> Self {
        Self {
            name: name.into(),
            datatype,
            lineno: 0,
            constant: false,
            not_null: false,
            default: None,
        }
    }
}
