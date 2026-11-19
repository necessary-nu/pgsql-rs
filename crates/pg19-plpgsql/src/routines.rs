use crate::body::Returns;
use crate::compile::compile_in;
use crate::signature::{Setup, Signature};
use crate::{
    Block, CompileError, DataType, Datum, EmbeddedSql, Error, NativeSql, Options, ParseMode,
    Statement, Variable,
};
use pg19_nodes::{CreateFunctionStmt, Node};
use serde_json::{json, Value};

#[derive(Debug, PartialEq, Eq)]
pub struct Routine {
    pub datums: Vec<Datum>,
    pub action: Option<Block>,
    pub old_varno: Option<usize>,
}

impl Routine {
    fn placeholder() -> Self {
        Self {
            datums: vec![],
            action: None,
            old_varno: None,
        }
    }
    pub fn json(&self) -> Value {
        let mut object = json!({"datums":self.datums.iter().enumerate().map(|(id,d)|crate::json::datum(d,id)).collect::<Vec<_>>()});
        if let Some(id) = self.old_varno {
            object["old_varno"] = json!(id);
        }
        if let Some(b) = &self.action {
            object["action"] = crate::json::block(b);
        }
        json!({"PLpgSQL_function":object})
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RoutineError {
    OuterSql(pg19_parser::Error),
    Body {
        error: CompileError,
        context: Option<String>,
    },
}

impl std::fmt::Display for RoutineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OuterSql(error) => error.fmt(f),
            Self::Body {
                error,
                context: Some(context),
            } => write!(f, "{error} ({context})"),
            Self::Body {
                error,
                context: None,
            } => error.fmt(f),
        }
    }
}

impl std::error::Error for RoutineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::OuterSql(error) => Some(error),
            Self::Body { error, .. } => Some(error),
        }
    }
}

impl RoutineError {
    fn plain(error: CompileError) -> Self {
        Self::Body {
            error,
            context: None,
        }
    }
    fn in_function(error: CompileError, name: &str, line: usize) -> Self {
        Self::Body {
            error,
            context: Some(format!(
                "compilation of PL/pgSQL function \"{name}\" near line {line}"
            )),
        }
    }
}

// [spec:pgq:req:plpgsql.routines]
pub fn parse_routines(source: &str, options: Options) -> Result<Vec<Routine>, RoutineError> {
    let nodes =
        pg19_parser::parse(source, ParseMode::Sql, options).map_err(RoutineError::OuterSql)?;
    compile_routines(&nodes, options, &mut NativeSql { options })
}

pub fn compile_routines(
    nodes: &[Node],
    options: Options,
    sql: &mut impl EmbeddedSql,
) -> Result<Vec<Routine>, RoutineError> {
    let mut routines = Vec::new();
    for n in nodes {
        let Node::RawStmt(raw) = n else {
            return Err(RoutineError::plain(CompileError::InvalidTree(
                "expected RawStmt",
            )));
        };
        let routine = match raw.stmt.as_deref() {
            Some(Node::DoStmt(d)) => {
                let (language, body) = arguments(&d.args, false).map_err(RoutineError::plain)?;
                let body = body.ok_or_else(|| {
                    RoutineError::plain(CompileError::from(Error::without_position(
                        "no inline code specified",
                        "42601",
                    )))
                })?;
                if language != "plpgsql" {
                    Routine::placeholder()
                } else {
                    finish(Setup::new(None), "inline_code_block", body, options, sql)?
                }
            }
            Some(Node::CreateFunctionStmt(f)) => function(f, options, sql)?,
            _ => continue,
        };
        routines.push(routine);
    }
    Ok(routines)
}

fn string(n: &Node) -> Result<&str, CompileError> {
    match n {
        Node::String(s) => Ok(s),
        _ => Err(CompileError::InvalidTree(
            "expected a string in routine options",
        )),
    }
}

fn arguments(options: &[Node], function: bool) -> Result<(&str, Option<&str>), CompileError> {
    let mut language = "plpgsql";
    let mut body = None;
    for n in options {
        let Node::DefElem(def) = n else {
            return Err(CompileError::InvalidTree(
                "expected DefElem in routine options",
            ));
        };
        match (def.defname.as_deref(), def.arg.as_deref()) {
            (Some("language"), Some(arg)) => language = string(arg)?,
            (Some("as"), Some(Node::List(parts))) if function => {
                for part in parts.iter() {
                    body = Some(string(part)?);
                }
            }
            (Some("as"), Some(arg)) if !function => body = Some(string(arg)?),
            (Some("as" | "language"), _) => {
                return Err(CompileError::InvalidTree("invalid routine option argument"))
            }
            _ => {}
        }
    }
    Ok((language, body))
}

fn function(
    f: &CreateFunctionStmt,
    options: Options,
    sql: &mut impl EmbeddedSql,
) -> Result<Routine, RoutineError> {
    let (lang, body) = arguments(&f.options, true).map_err(RoutineError::plain)?;
    if lang != "plpgsql" {
        return Ok(Routine::placeholder());
    }
    let message = match (f.sql_body.is_some(), body) {
        (true, Some(_)) => Some("duplicate function body specified"),
        (true, None) => Some("inline SQL function body only valid for language SQL"),
        (false, None) => Some("no function body specified"),
        _ => None,
    };
    if let Some(m) = message {
        return Err(RoutineError::plain(CompileError::from(
            Error::without_position(m, "42P13"),
        )));
    }
    let name = f
        .funcname
        .first()
        .ok_or(CompileError::InvalidTree("missing function name"))
        .and_then(string)
        .map_err(RoutineError::plain)?;
    let sig = Signature::read(f).map_err(RoutineError::plain)?;
    let setup = sig
        .setup(name)
        .map_err(|e| RoutineError::in_function(e, name, 1))?;
    finish(setup, name, body.unwrap(), options, sql)
}

fn finish(
    mut setup: Setup,
    name: &str,
    source: &str,
    options: Options,
    sql: &mut impl EmbeddedSql,
) -> Result<Routine, RoutineError> {
    setup
        .env
        .declare(Variable::implicit("found", DataType::Scalar("bool".into())));
    let result = compile_in(source, options, setup.env, setup.returns, sql)
        .map_err(|(e, line)| RoutineError::in_function(e, name, line))?;
    let mut block = result.block;
    if setup.returns != Returns::Value {
        if block.label.is_some() || !block.exceptions.is_empty() {
            block = Block {
                lineno: 0,
                label: None,
                exceptions: Vec::new(),
                body: vec![Statement::Block(block)],
            };
        }
        if !matches!(block.body.last(), Some(Statement::Return { .. })) {
            let datum = match setup.returns {
                Returns::Set { out } => out,
                Returns::Out(id) => Some(id),
                _ => None,
            };
            block.body.push(Statement::Return {
                lineno: 0,
                datum,
                expression: None,
            });
        }
    }
    Ok(Routine {
        action: Some(block),
        datums: result.datums,
        old_varno: setup.old_varno,
    })
}

impl RoutineError {
    // [spec:pgq:req:plpgsql.errors]
    pub fn diagnostic(&self) -> Option<(&Error, Option<&str>)> {
        match self {
            Self::OuterSql(pg19_parser::Error::Diagnostic(e)) => Some((e, None)),
            Self::Body {
                error: CompileError::Diagnostic(e),
                context,
            } => Some((e, context.as_deref())),
            _ => None,
        }
    }
}
