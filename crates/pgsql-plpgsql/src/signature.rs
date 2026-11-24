use crate::body::Returns;
use crate::scope::Environment;
use crate::{Builtin, CompileError, DataType, Datum, Error, Variable};
use pgsql_nodes::{CreateFunctionStmt, FunctionParameterMode as Mode, Node};

pub(crate) struct Setup {
    pub env: Environment,
    pub returns: Returns,
    pub old_varno: Option<usize>,
}

fn named(name: &str) -> Builtin {
    Builtin::named(name).expect("PG19 built-in type from the pinned catalog")
}

fn validator_type(t: Builtin) -> Builtin {
    match t.name {
        "anyelement" | "anynonarray" | "anyenum" | "anycompatible" | "anycompatiblenonarray" => {
            named("int4")
        }
        "anyarray" | "anycompatiblearray" => named("_int4"),
        "anyrange" | "anycompatiblerange" => named("int4range"),
        "anymultirange" | "anycompatiblemultirange" => named("int4multirange"),
        _ => t,
    }
}

#[derive(Clone)]
struct Parameter<'a> {
    name: Option<&'a str>,
    db_type: Builtin,
    input: bool,
    output: bool,
}

pub(crate) struct Signature<'a> {
    params: Vec<Parameter<'a>>,
    result: Builtin,
    procedure: bool,
    set: bool,
}

impl<'a> Signature<'a> {
    pub fn read(stmt: &'a CreateFunctionStmt) -> Result<Self, CompileError> {
        let mut params: Vec<Parameter> = Vec::new();
        let mut variadic = false;
        let mut defaulted = false;
        let mut outputs = Vec::new();
        for n in &stmt.parameters {
            let Node::FunctionParameter(p) = n else {
                return Err(CompileError::InvalidTree("expected function parameter"));
            };
            let t = p
                .arg_type
                .as_deref()
                .ok_or(CompileError::InvalidTree("missing parameter type"))?;
            let db_type = Builtin::resolve(t)?;
            let input = !matches!(p.mode, Mode::FUNC_PARAM_OUT | Mode::FUNC_PARAM_TABLE);
            let output = matches!(
                p.mode,
                Mode::FUNC_PARAM_OUT | Mode::FUNC_PARAM_TABLE | Mode::FUNC_PARAM_INOUT
            );
            if t.setof {
                return Err(CompileError::from(Error::without_position(
                    if stmt.is_procedure {
                        "procedures cannot accept set arguments"
                    } else {
                        "functions cannot accept set arguments"
                    },
                    "42P13",
                )));
            }
            if variadic && (input || stmt.is_procedure) {
                return Err(CompileError::from(Error::without_position(
                    if input {
                        "VARIADIC parameter must be the last input parameter"
                    } else {
                        "VARIADIC parameter must be the last parameter"
                    },
                    "42P13",
                )));
            }
            if p.mode == Mode::FUNC_PARAM_VARIADIC {
                // The standalone SearchSysCache1 tuple leaves typelem and typsubscript zero.
                if !matches!(db_type.name, "any" | "anyarray" | "anycompatiblearray") {
                    return Err(CompileError::from(Error::without_position(
                        "VARIADIC parameter must be an array",
                        "42P13",
                    )));
                }
                variadic = true;
            }
            let name = p.name.as_deref().filter(|s| !s.is_empty());
            if let Some(s) = name {
                if params
                    .iter()
                    .any(|q| q.name == Some(s) && ((q.input && input) || (q.output && output)))
                {
                    return Err(CompileError::from(Error::without_position(
                        format!("parameter name \"{s}\" used more than once"),
                        "42P13",
                    )));
                }
            }
            if p.defexpr.is_some() {
                if !input {
                    return Err(CompileError::from(Error::without_position(
                        "only input parameters can have default values",
                        "42P13",
                    )));
                }
                defaulted = true;
            } else if defaulted && input {
                return Err(CompileError::from(Error::without_position(
                    "input parameters after one with a default value must also have defaults",
                    "42P13",
                )));
            } else if defaulted && stmt.is_procedure {
                return Err(CompileError::from(Error::without_position(
                    "procedure OUT parameters cannot appear after one with a default value",
                    "42P13",
                )));
            }
            if output {
                outputs.push(db_type);
            }
            params.push(Parameter {
                name,
                db_type,
                input,
                output,
            });
        }
        let required = match outputs.as_slice() {
            [] => None,
            [t] if !stmt.is_procedure => Some(*t),
            _ => Some(named("record")),
        };
        let result = match &stmt.return_type {
            Some(t) => {
                let result = Builtin::resolve(t)?;
                if let Some(needed) = required.filter(|t| *t != result) {
                    return Err(CompileError::from(Error::without_position(
                        format!(
                            "function result type must be {} because of OUT parameters",
                            needed.sql_name()
                        ),
                        "42P13",
                    )));
                }
                result
            }
            None if stmt.is_procedure => required.unwrap_or(named("void")),
            None => required.ok_or_else(|| {
                CompileError::from(Error::without_position(
                    "function result type must be specified",
                    "42P13",
                ))
            })?,
        };
        Ok(Self {
            params,
            result,
            procedure: stmt.is_procedure,
            set: stmt.return_type.as_ref().is_some_and(|t| t.setof),
        })
    }

    pub fn setup(self, name: &str) -> Result<Setup, CompileError> {
        let mut setup = Setup::new(Some(name));
        if matches!(self.result.name, "trigger" | "event_trigger") {
            if self.params.iter().any(|p| p.input) {
                return Err(CompileError::from(Error::without_position(
                    if self.result.name == "trigger" {
                        "trigger functions cannot have declared arguments"
                    } else {
                        "event trigger functions cannot have declared arguments"
                    },
                    "42P13",
                )));
            }
            setup.add_trigger(self.result.name == "trigger");
            return Ok(setup);
        }
        let mut outs = Vec::new();
        for (i, p) in self.params.iter().enumerate() {
            let pos = format!("${}", i + 1);
            let arg = p.name.unwrap_or(&pos);
            let t = validator_type(p.db_type).datatype();
            if let DataType::Pseudo(ty) = t {
                return Err(CompileError::from(Error::without_position(
                    format!("PL/pgSQL functions cannot accept type {ty}"),
                    "0A000",
                )));
            }
            if setup.env.local(&pos) || setup.env.local(arg) {
                return Err(CompileError::from(Error::without_position(
                    format!("parameter name \"{arg}\" used more than once"),
                    "42P13",
                )));
            }
            setup.env.declare(Variable::implicit(arg, t));
            if p.output {
                outs.push((arg.to_owned(), i));
            }
            if arg != pos {
                setup.env.alias(pos, i);
            }
        }
        let out = match outs.as_slice() {
            [] => None,
            [(_, id)] if !self.procedure => Some(*id),
            _ => {
                let index = setup.env.datums.len();
                setup.env.datums.push(Datum::Row {
                    lineno: -1,
                    fields: outs,
                });
                Some(index)
            }
        };
        let actual = validator_type(self.result);
        if actual != self.result && out.is_none() {
            setup
                .env
                .declare(Variable::implicit("$0", actual.datatype()));
        }
        if actual.kind == b'p' && !matches!(actual.name, "void" | "record") {
            return Err(CompileError::from(Error::without_position(
                format!("PL/pgSQL functions cannot return type {}", actual.name),
                "0A000",
            )));
        }
        setup.returns = if self.set {
            Returns::Set { out }
        } else if actual.name == "void" {
            Returns::Void {
                procedure: self.procedure,
            }
        } else if let Some(id) = out {
            Returns::Out(id)
        } else {
            Returns::Value
        };
        Ok(setup)
    }
}

impl Setup {
    pub fn new(name: Option<&str>) -> Self {
        let mut env = Environment::default();
        env.push(name.map(str::to_owned));
        Self {
            env,
            returns: Returns::Void { procedure: false },
            old_varno: None,
        }
    }

    fn add_trigger(&mut self, dml: bool) {
        let promises = if dml {
            self.env
                .declare(Variable::implicit("new", DataType::Record));
            self.env
                .declare(Variable::implicit("old", DataType::Record));
            self.old_varno = Some(1);
            self.returns = Returns::Value;
            &[
                ("tg_name", "name", 1),
                ("tg_when", "text", 2),
                ("tg_level", "text", 3),
                ("tg_op", "text", 4),
                ("tg_relid", "oid", 5),
                ("tg_relname", "name", 6),
                ("tg_table_name", "name", 6),
                ("tg_table_schema", "name", 7),
                ("tg_nargs", "int4", 8),
                ("tg_argv", "_text", 9),
            ][..]
        } else {
            &[("tg_event", "text", 10), ("tg_tag", "text", 11)][..]
        };
        for &(name, t, promise) in promises {
            let variable = Variable::implicit(name, named(t).datatype());
            self.env.alias(name.to_owned(), self.env.datums.len());
            self.env.datums.push(Datum::Promise { variable, promise });
        }
    }
}
