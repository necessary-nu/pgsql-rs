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
    Cursor {
        variable: Variable,
        query: Expression,
        args: Option<usize>,
        options: i32,
    },
    Promise {
        variable: Variable,
        promise: u8,
    },
    Row {
        lineno: i32,
        fields: Vec<(String, usize)>,
    },
    RecordField {
        name: String,
        parent: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub lineno: usize,
    pub label: Option<String>,
    pub body: Vec<Statement>,
    pub exceptions: Vec<ExceptionHandler>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExceptionHandler {
    pub conditions: Vec<String>,
    pub action: Vec<Statement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Elsif {
    pub lineno: usize,
    pub condition: Expression,
    pub body: Vec<Statement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub id: usize,
    pub variable: Datum,
    pub strict: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForSource {
    Integer {
        lower: Expression,
        upper: Expression,
        step: Option<Expression>,
        reverse: bool,
    },
    Query {
        query: Expression,
        dynamic: bool,
        params: Vec<Expression>,
    },
    Cursor {
        cursor: usize,
        argquery: Option<Expression>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchDirection {
    pub direction: u8,
    pub count: i64,
    pub expression: Option<Expression>,
    pub multiple: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticItem {
    pub name: &'static str,
    pub target: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CursorQuery {
    Bound(Option<Expression>),
    Static(Expression),
    Dynamic(Expression, Vec<Expression>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Statement {
    Case {
        lineno: usize,
        operand: Option<Expression>,
        datum: usize,
        branches: Vec<Elsif>,
        otherwise: Option<Vec<Statement>>,
    },
    Foreach {
        lineno: usize,
        label: Option<String>,
        datum: usize,
        slice: i32,
        expression: Expression,
        body: Vec<Statement>,
    },
    GetDiagnostics {
        lineno: usize,
        stacked: bool,
        items: Vec<DiagnosticItem>,
    },
    Fetch {
        lineno: usize,
        cursor: usize,
        target: Option<Box<Target>>,
        direction: FetchDirection,
    },
    Open {
        lineno: usize,
        cursor: usize,
        options: i32,
        query: CursorQuery,
    },
    Close {
        lineno: usize,
        cursor: usize,
    },
    Block(Block),
    For {
        lineno: usize,
        label: Option<String>,
        target: Box<Target>,
        source: Box<ForSource>,
        body: Vec<Statement>,
    },
    Execute {
        lineno: usize,
        query: Expression,
        target: Option<Box<Target>>,
        params: Vec<Expression>,
    },
    If {
        lineno: usize,
        condition: Expression,
        then_body: Vec<Statement>,
        elsif: Vec<Elsif>,
        else_body: Vec<Statement>,
    },
    Loop {
        lineno: usize,
        label: Option<String>,
        condition: Option<Expression>,
        body: Vec<Statement>,
    },
    Exit {
        lineno: usize,
        is_exit: bool,
        label: Option<String>,
        condition: Option<Expression>,
    },
    Raise {
        lineno: usize,
        level: u8,
        condition: Option<String>,
        message: Option<String>,
        params: Vec<Expression>,
        options: Vec<(u8, Expression)>,
    },
    Assert {
        lineno: usize,
        condition: Expression,
        message: Option<Expression>,
    },
    ExecSql {
        lineno: usize,
        target: Option<Box<Target>>,
        expression: Expression,
    },
    Call {
        lineno: usize,
        is_call: bool,
        expression: Expression,
    },
    Transaction {
        lineno: usize,
        commit: bool,
        chain: bool,
    },
    ReturnNext {
        lineno: usize,
        datum: Option<usize>,
        expression: Option<Expression>,
    },
    ReturnQuery {
        lineno: usize,
        query: Expression,
        dynamic: bool,
        params: Vec<Expression>,
    },
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
    SqlAction(pgsql_grammar::Error),
    InvalidTree(&'static str),
    Unsupported { production: &'static str },
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Diagnostic(error) => error.fmt(f),
            Self::SqlAction(error) => error.fmt(f),
            Self::InvalidTree(reason) => write!(f, "invalid PL/pgSQL tree: {reason}"),
            Self::Unsupported { production } => {
                write!(f, "unsupported PL/pgSQL production: {production}")
            }
        }
    }
}

impl std::error::Error for CompileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Diagnostic(error) => Some(error),
            Self::SqlAction(error) => Some(error),
            Self::InvalidTree(_) | Self::Unsupported { .. } => None,
        }
    }
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

impl From<pgsql_grammar::Error> for CompileError {
    fn from(e: pgsql_grammar::Error) -> Self {
        match e {
            pgsql_grammar::Error::Diagnostic(diagnostic) => Self::Diagnostic(diagnostic),
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
