mod control;
mod exception;
mod execsql;
mod loops;
mod target;
use crate::body::Returns;
use crate::scope::Environment;
use crate::Builtin;
use crate::{
    token, Actions, Block, BodyScanner, CompileError, CompiledBody, DataType, EmbeddedSql, Error,
    Expression, Input, Lookup, Namespace, Options, ParseMode, Production, Reduction, Resolved,
    Statement, Token, Variable,
};

/// Compile the grammar block before the routine compiler adds any implicit RETURN.
/// Parameters and implicit variables appear in their caller-supplied datum order.
pub fn compile_block(
    source: &str,
    options: Options,
    routine_name: Option<String>,
    initial: Vec<Variable>,
    sql: &mut impl EmbeddedSql,
) -> Result<CompiledBody, CompileError> {
    let mut env = Environment::default();
    env.push(routine_name);
    for v in initial {
        env.declare(v);
    }
    compile_in(source, options, env, Returns::Value, sql).map_err(|(e, _)| e)
}

pub(crate) fn compile_in(
    source: &str,
    options: Options,
    env: Environment,
    returns: Returns,
    sql: &mut impl EmbeddedSql,
) -> Result<CompiledBody, (CompileError, usize)> {
    let mut compiler = Compiler { env, returns, sql };
    let mut input = Input {
        scanner: BodyScanner::new(source, options),
        lookahead: None,
    };
    let value = crate::driver::drive(&mut input, &mut compiler)
        .map_err(|e| (e, input.scanner.latest_line()))?;
    let Reduced::Statement(Statement::Block(block)) = value else {
        unreachable!("pl_function returns its pl_block")
    };
    Ok(CompiledBody {
        datums: compiler.env.datums,
        block,
    })
}

#[derive(Default)]
enum Reduced {
    #[default]
    None,
    Token(Token),
    Type(DataType),
    Expression(Expression),
    Statement(Statement),
    List(Vec<Statement>),
    Datum(usize),
    LoopVariable(Token, Option<Box<crate::Target>>),
    For(Box<crate::Target>, Box<crate::ForSource>),
    Handlers(Vec<crate::ExceptionHandler>),
    Conditions(Vec<String>),
    Elsifs(Vec<crate::Elsif>),
    LoopBody {
        body: Vec<Statement>,
        end: Option<String>,
        at: usize,
    },
}

impl Reduced {
    fn token(self) -> Token {
        let Self::Token(t) = self else {
            unreachable!("grammar requires a token")
        };
        t
    }
    fn name(self) -> Option<String> {
        match self {
            Self::None => None,
            Self::Token(t) => Some(
                t.names
                    .into_iter()
                    .next()
                    .expect("grammar requires an identifier"),
            ),
            _ => unreachable!("grammar requires an optional identifier"),
        }
    }
    fn expr(self) -> Expression {
        let Self::Expression(e) = self else {
            unreachable!("grammar requires an expression")
        };
        e
    }

    fn list(self) -> Vec<Statement> {
        match self {
            Self::List(v) => v,
            Self::None => Vec::new(),
            _ => unreachable!("expected procedure section"),
        }
    }
}

struct Compiler<'a, S> {
    env: Environment,
    returns: Returns,
    sql: &'a mut S,
}

impl<S> Namespace for Compiler<'_, S> {
    fn lookup(&mut self, names: &[String]) -> Option<Resolved> {
        self.env.lookup(names)
    }
}

impl<S: EmbeddedSql> Compiler<'_, S> {
    fn datatype(&mut self, input: &mut Input<'_>) -> Result<Reduced, CompileError> {
        let scanner = &mut input.scanner;
        let mut token = match input.lookahead.take() {
            Some(t) => t,
            None => scanner.next_token(self)?,
        };
        let start = token.span.start;
        let mut depth = 0;
        loop {
            match token.id {
                59
                | 61
                | token::K_COLLATE
                | token::K_NOT
                | token::K_DEFAULT
                | token::COLON_EQUALS => break,
                44 | 41 if depth == 0 => break,
                40 => depth += 1,
                41 => depth -= 1,
                0 => {
                    return Err(scanner
                        .error(if depth == 0 {
                            "incomplete data type declaration"
                        } else {
                            "mismatched parentheses"
                        })
                        .into())
                }
                _ => {}
            }
            token = scanner.next_token(self)?;
        }
        let source = scanner.source();
        let type_name = &source[start..token.span.start];
        if type_name.is_empty() {
            return Err(scanner.error("missing data type declaration").into());
        }
        scanner.push_back(token);
        let datatype = self
            .sql
            .datatype(type_name)
            .map_err(|e| e.in_body(source, start))?;
        Ok(Reduced::Type(datatype))
    }

    fn declaration(
        &mut self,
        rhs: &mut Reduction<Reduced>,
        s: &BodyScanner,
    ) -> Result<Reduced, CompileError> {
        let name = rhs.take(1).name().unwrap();
        let constant = matches!(rhs.value(2), Reduced::Token(_));
        let Reduced::Type(datatype) = rhs.take(3) else {
            unreachable!("missing declaration type")
        };
        if let DataType::Pseudo(pseudo) = &datatype {
            return Err(Error::without_position(
                format!("variable \"{name}\" has pseudo-type {pseudo}"),
                "0A000",
            )
            .into());
        }
        if matches!(rhs.value(4), Reduced::Token(_)) {
            let ty = match &datatype {
                DataType::Scalar(name) => Builtin::named(name),
                _ => None,
            };
            if !ty.is_some_and(|t| t.collatable) {
                let name = ty.map_or("record", Builtin::sql_name);
                return Err(Error::at(
                    s.source(),
                    rhs.location(4),
                    format!("collations are not supported by type {name}"),
                    "42804",
                )
                .into());
            }
        }
        let not_null = matches!(rhs.value(5), Reduced::Token(_));
        let default = match rhs.take(6) {
            Reduced::Expression(e) => Some(e),
            Reduced::None => None,
            _ => unreachable!("invalid default"),
        };
        if not_null && default.is_none() {
            return Err(Error::at(
                s.source(),
                rhs.location(5),
                format!(
                    "variable \"{name}\" must have a default value, since it's declared NOT NULL"
                ),
                "22004",
            )
            .into());
        }
        self.env.declare(Variable {
            name,
            lineno: s.line(rhs.location(1)),
            datatype,
            constant,
            not_null,
            default,
        });
        Ok(Reduced::None)
    }

    fn expression(
        &mut self,
        s: &mut BodyScanner,
        mode: ParseMode,
        perform: bool,
    ) -> Result<Expression, CompileError> {
        let f = s.read_sql(self, &[59], ";", mode, mode == ParseMode::PlPgSqlExpr)?;
        let query = if perform {
            format!("SELECT{}", &f.query[7..])
        } else {
            f.query.to_owned()
        };
        self.sql
            .validate(&query, mode)
            .map_err(|e| e.in_body(s.source(), f.span.start + usize::from(perform)))?;
        Ok(Expression { query, mode })
    }

    fn return_stmt(
        &mut self,
        location: usize,
        s: &mut BodyScanner,
    ) -> Result<Reduced, CompileError> {
        let first = s.next_token(self)?;
        if first.id == 0 {
            return Err(s.error("unexpected end of function definition").into());
        }
        if first.is_keyword(token::K_NEXT, "next") || first.is_keyword(token::K_QUERY, "query") {
            return self.return_set(location, s, first.is_keyword(token::K_NEXT, "next"));
        }
        let lineno = s.line(location);
        let mut datum = None;
        let mut expression = None;
        match self.returns {
            Returns::Value => {
                if let Some(id) = first
                    .datum
                    .filter(|id| !matches!(self.env.datums[*id], crate::Datum::RecordField { .. }))
                {
                    if s.peek_raw(0)?.id == 59 {
                        datum = Some(id);
                        s.next_token(self)?;
                    }
                }
                if datum.is_none() {
                    s.push_back(first);
                    expression = Some(self.expression(s, ParseMode::PlPgSqlExpr, false)?);
                }
            }
            mode => {
                if first.id != 59 {
                    let (message, state) = match mode {
                        Returns::Void { procedure: true } => {
                            ("RETURN cannot have a parameter in a procedure", "42601")
                        }
                        Returns::Void { procedure: false } => (
                            "RETURN cannot have a parameter in function returning void",
                            "42804",
                        ),
                        Returns::Out(_) => (
                            "RETURN cannot have a parameter in function with OUT parameters",
                            "42804",
                        ),
                        _ => (
                            "RETURN cannot have a parameter in function returning set",
                            "42804",
                        ),
                    };
                    return Err(Error::at(s.source(), first.span.start, message, state).into());
                }
                if let Returns::Out(id) = mode {
                    datum = Some(id);
                }
            }
        }
        Ok(Reduced::Statement(Statement::Return {
            lineno,
            datum,
            expression,
        }))
    }

    fn assignment(&mut self, t: Token, s: &mut BodyScanner) -> Result<Reduced, CompileError> {
        let id = t.datum.unwrap();
        let var = self.env.variable(id);
        if var.constant {
            return Err(Error::at(
                s.source(),
                t.span.start,
                format!("variable \"{}\" is declared CONSTANT", var.name),
                "22005",
            )
            .into());
        }
        let mode = match t.names.len() {
            1 => ParseMode::PlPgSqlAssign1,
            2 => ParseMode::PlPgSqlAssign2,
            3 => ParseMode::PlPgSqlAssign3,
            _ => unreachable!("expected one to three names"),
        };
        let lineno = s.line(t.span.start);
        s.push_back(t);
        let expression = self.expression(s, mode, false)?;
        Ok(Reduced::Statement(Statement::Assign {
            lineno,
            datum: id,
            expression,
        }))
    }
}

fn check_label(
    label: Option<&str>,
    end: &str,
    at: usize,
    source: &str,
) -> Result<(), CompileError> {
    let message = match label {
        None => Some(format!("end label \"{end}\" specified for unlabeled block")),
        Some(s) if s != end => Some(format!(
            "end label \"{end}\" differs from block's label \"{s}\""
        )),
        Some(_) => None,
    };
    if let Some(m) = message {
        return Err(Error::at(source, at, m, "42601").into());
    }

    Ok(())
}

fn block(rhs: &mut Reduction<Reduced>, scanner: &BodyScanner) -> Result<Reduced, CompileError> {
    let label = rhs.take(1).name();
    if let Some(end) = rhs.take(6).name() {
        check_label(label.as_deref(), &end, rhs.location(6), scanner.source())?;
    }
    Ok(Reduced::Statement(Statement::Block(Block {
        lineno: scanner.line(rhs.location(2)),
        label,
        body: rhs.take(3).list(),
        exceptions: match rhs.take(4) {
            Reduced::Handlers(v) => v,
            Reduced::None => Vec::new(),
            _ => unreachable!(),
        },
    })))
}

impl<S: EmbeddedSql> Actions for Compiler<'_, S> {
    type Value = Reduced;
    type Error = CompileError;
    fn shift(&mut self, token: Token) -> Reduced {
        Reduced::Token(token)
    }
    fn reduce(
        &mut self,
        rule: Production,
        rhs: &mut Reduction<Reduced>,
        input: &mut Input,
    ) -> Result<Reduced, CompileError> {
        let sig = rule.info().production;
        let (lhs, production) = sig.split_once(": ").unwrap();
        let s = &mut input.scanner;
        match lhs {
            "pl_function" => Ok(rhs.take(2)),
            "opt_block_label" => {
                let value = if production == "%empty" {
                    Reduced::None
                } else {
                    rhs.take(2)
                };
                let label = match &value {
                    Reduced::Token(t) => t.names.first().cloned(),
                    _ => None,
                };
                self.env.push(label);
                Ok(value)
            }
            "decl_sect" => {
                s.lookup = Lookup::Normal;
                Ok(rhs.take(1))
            }
            "decl_start" => {
                s.lookup = Lookup::Declare;
                Ok(Reduced::None)
            }
            "decl_varname" => {
                s.line(rhs.location(1));
                let t = rhs.take(1).token();
                if self.env.local(&t.names[0]) {
                    return Err(s.error("duplicate declaration").into());
                }
                Ok(Reduced::Token(t))
            }
            "decl_aliasitem" => {
                let t = rhs.take(1).token();
                let name = t.names.join(".");
                let id = self
                    .env
                    .lookup(&t.names)
                    .ok_or_else(|| {
                        Error::at(
                            s.source(),
                            t.span.start,
                            format!("variable \"{name}\" does not exist"),
                            "42704",
                        )
                    })?
                    .datum;
                Ok(Reduced::Datum(id))
            }
            "decl_statement" if production.contains("K_ALIAS") => {
                let name = rhs.take(1).name().unwrap();
                let Reduced::Datum(id) = rhs.take(4) else {
                    unreachable!()
                };
                self.env.alias(name, id);
                Ok(Reduced::None)
            }
            "decl_collate" if production != "%empty" => Ok(rhs.take(1)),
            "decl_datatype" => self.datatype(input),
            "decl_defval" if production == "decl_defkey" => Ok(Reduced::Expression(
                self.expression(s, ParseMode::PlPgSqlExpr, false)?,
            )),
            "decl_statement" if production.starts_with("decl_varname decl_const ") => {
                self.declaration(rhs, s)
            }
            "pl_block" => {
                self.env.pop();
                block(rhs, s)
            }
            "proc_sect" if production != "%empty" => {
                let mut list = rhs.take(1).list();
                if let Reduced::Statement(stmt) = rhs.take(2) {
                    list.push(stmt);
                }
                Ok(Reduced::List(list))
            }
            "for_variable" => self.for_variable(rhs.take(1).token(), s),
            "for_control" => self.for_control(rhs, s),
            "stmt_for" => self.finish_for(rhs, s),
            "stmt_dynexecute" => self.dynamic_execute(rhs.location(1), s),
            "stmt_raise" => self.raise(rhs.location(1), s),
            "@2" | "proc_condition" | "proc_conditions" | "proc_exception" | "proc_exceptions"
            | "exception_sect"
                if production != "%empty" || lhs == "@2" =>
            {
                self.exceptions(lhs, production, rhs, s)
            }
            "stmt_return" => self.return_stmt(rhs.location(1), s),
            "stmt_assign" => self.assignment(rhs.take(1).token(), s),
            "stmt_execsql" => self.execsql(rhs.take(1).token(), s),
            "stmt_call" => {
                let t = rhs.take(1).token();
                let lineno = s.line(t.span.start);
                let is_call = t.id == token::K_CALL;
                s.push_back(t);
                Ok(Reduced::Statement(Statement::Call {
                    lineno,
                    is_call,
                    expression: self.expression(s, ParseMode::Sql, false)?,
                }))
            }
            "stmt_perform" => {
                let t = rhs.take(1).token();
                let lineno = s.line(t.span.start);
                s.push_back(t);
                Ok(Reduced::Statement(Statement::Perform {
                    lineno,
                    expression: self.expression(s, ParseMode::Sql, true)?,
                }))
            }
            "any_identifier" if matches!(rhs.value(1), Reduced::Token(t) if t.names.len() > 1) => {
                Err(s.error("syntax error").into())
            }
            "opt_label" | "decl_const" | "decl_notnull" | "any_identifier"
            | "unreserved_keyword" | "proc_stmt" => Ok(rhs.default_value()),
            "decl_defval" | "stmt_null" | "proc_sect" | "decl_stmt"
                if matches!(production, "';'" | "K_NULL ';'" | "%empty" | "K_DECLARE") =>
            {
                Ok(Reduced::None)
            }
            "exception_sect" | "decl_collate" if production == "%empty" => Ok(Reduced::None),
            _ => self.control(lhs, production, rhs, s, sig),
        }
    }
}
