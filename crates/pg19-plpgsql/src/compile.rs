use crate::scope::Environment;
use crate::{
    parse_body, token, Actions, Block, BodyScanner, CompileError, CompiledBody, DataType,
    EmbeddedSql, Error, Expression, Input, Lookup, Namespace, Options, ParseMode, Production,
    Reduction, Resolved, Statement, Token, Variable,
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
    let mut compiler = Compiler { env, sql };
    let value = parse_body(source, options, &mut compiler)?;
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
    sql: &'a mut S,
}

impl<S> Namespace for Compiler<'_, S> {
    fn lookup(&mut self, names: &[String]) -> Option<Resolved> {
        self.env.lookup(names)
    }
}

fn transpose(mut e: Error, source: &str, start: usize) -> Error {
    e.position = source[..start].chars().count() + e.position.max(1);
    e
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
            .map_err(|e| transpose(e, source, start))?;
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
            .map_err(|e| transpose(e, s.source(), f.span.start + usize::from(perform)))?;
        Ok(Expression { query, mode })
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

fn block(rhs: &mut Reduction<Reduced>, scanner: &BodyScanner) -> Result<Reduced, CompileError> {
    let label = rhs.take(1).name();
    if let Some(end) = rhs.take(6).name() {
        let message = match &label {
            None => Some(format!("end label \"{end}\" specified for unlabeled block")),
            Some(s) if *s != end => Some(format!(
                "end label \"{end}\" differs from block's label \"{s}\""
            )),
            Some(_) => None,
        };
        if let Some(m) = message {
            return Err(Error::at(scanner.source(), rhs.location(6), m, "42601").into());
        }
    }
    Ok(Reduced::Statement(Statement::Block(Block {
        lineno: scanner.line(rhs.location(2)),
        label,
        body: rhs.take(3).list(),
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
                let t = rhs.take(1).token();
                if self.env.local(&t.names[0]) {
                    return Err(s.error("duplicate declaration").into());
                }
                Ok(Reduced::Token(t))
            }
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
            "stmt_assign" => self.assignment(rhs.take(1).token(), s),
            "stmt_execsql" if matches!(production, "T_WORD" | "T_CWORD") => {
                let t = rhs.take(1).token();
                if matches!(s.peek_raw(0)?.id, 61 | token::COLON_EQUALS | 91 | 46) {
                    let name = t.names.join(".");
                    Err(Error::at(
                        s.source(),
                        t.span.start,
                        format!("\"{name}\" is not a known variable"),
                        "42601",
                    )
                    .into())
                } else {
                    Err(CompileError::Unsupported { production: sig })
                }
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
            _ => Err(CompileError::Unsupported { production: sig }),
        }
    }
}
