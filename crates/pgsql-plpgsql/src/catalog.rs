use crate::{token, BodyScanner, CompileError, DataType, EmbeddedSql, Error, Options, ParseMode};
use pgsql_nodes::{Node, TypeName};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Builtin {
    pub oid: u32,
    pub name: &'static str,
    pub kind: u8,
    pub array_oid: u32,
    pub collatable: bool,
}

pub const BUILTINS: &[Builtin] = include!("../data/types.in");

impl Builtin {
    pub fn named(name: &str) -> Option<Self> {
        BUILTINS
            .binary_search_by_key(&name, |t| t.name)
            .ok()
            .map(|i| BUILTINS[i])
    }
    pub fn by_oid(oid: u32) -> Option<Self> {
        BUILTINS.iter().find(|t| t.oid == oid).copied()
    }

    pub fn sql_name(self) -> &'static str {
        match self.name {
            "int4" => "integer",
            "int2" => "smallint",
            "int8" => "bigint",
            "float4" => "real",
            "float8" => "double precision",
            "bool" => "boolean",
            "varchar" => "character varying",
            "bpchar" => "character",
            "timestamptz" => "timestamp with time zone",
            "timestamp" => "timestamp without time zone",
            "time" => "time without time zone",
            "timetz" => "time with time zone",
            other => other,
        }
    }

    pub fn resolve(t: &TypeName) -> Result<Self, CompileError> {
        if t.pct_type {
            return Err(Error::without_position("Not implemented", "XX000").into());
        }
        let names: Vec<_> = t
            .names
            .iter()
            .map(|n| match n {
                Node::String(s) => Ok(s.as_str()),
                _ => Err(CompileError::InvalidTree(
                    "expected a string in TypeName.names",
                )),
            })
            .collect::<Result<_, _>>()?;
        let missing = || {
            Error::without_position(
                format!("type \"{}\" does not exist", names.join(".")),
                "42704",
            )
        };
        let record = || Self::named("record").unwrap();
        let base =
            match names.as_slice() {
                [] => Self::by_oid(t.type_oid).ok_or_else(missing)?,
                [name] => Self::named(name).unwrap_or_else(record),
                ["public", _] | [_, "public", _] => record(),
                ["pg_catalog", name] | [_, "pg_catalog", name] => {
                    Self::named(name).ok_or_else(missing)?
                }
                [_, _] | [_, _, _] => return Err(Error::without_position(
                    "Not implemented (LookupExplicitNamespace only supports pg_catalog and public)",
                    "XX000",
                )
                .into()),
                _ => {
                    return Err(Error::without_position(
                        format!(
                            "improper qualified name (too many dotted names): {}",
                            names.join(".")
                        ),
                        "42601",
                    )
                    .into())
                }
            };
        if t.array_bounds.is_empty() {
            Ok(base)
        } else {
            Ok(Self::by_oid(base.array_oid).ok_or_else(missing)?)
        }
    }

    pub fn datatype(self) -> DataType {
        match (self.kind, self.name) {
            (b'c', _) | (_, "record") => DataType::Record,
            (b'p', _) => DataType::Pseudo(self.name.into()),
            _ => DataType::Scalar(self.name.into()),
        }
    }
}

#[derive(Default)]
pub struct NativeSql {
    pub options: Options,
}

impl EmbeddedSql for NativeSql {
    fn datatype(&mut self, source: &str) -> Result<DataType, CompileError> {
        if source.contains('%') {
            let mut scanner = BodyScanner::new(source, self.options);
            let name = scanner.next_token(&mut ())?;
            if !name.names.is_empty() && scanner.next_token(&mut ())?.id == u16::from(b'%') {
                let suffix = match scanner.next_token(&mut ())?.id {
                    token::K_TYPE => Some("TYPE"),
                    token::K_ROWTYPE => Some("rowtype"),
                    _ => None,
                };
                if let Some(suffix) = suffix {
                    let mut t = scanner.next_token(&mut ())?;
                    let mut array = t.id == token::K_ARRAY;
                    if array {
                        t = scanner.next_token(&mut ())?;
                    }
                    while t.id == 91 {
                        array = true;
                        t = scanner.next_token(&mut ())?;
                        if t.id == token::ICONST {
                            t = scanner.next_token(&mut ())?;
                        }
                        if t.id != 93 {
                            return Err(scanner.error("syntax error, expected \"]\"").into());
                        }
                        t = scanner.next_token(&mut ())?;
                    }
                    if array {
                        return Err(Error::without_position("Not implemented (SearchSysCache1 got TYPEOID cache request for type OID 0)","XX000").into());
                    }
                    if t.id != 0 {
                        return Err(scanner.error("syntax error").into());
                    }
                    return Ok(DataType::Scalar(format!(
                        "{}%{suffix}",
                        name.names.join(".")
                    )));
                }
            }
        }
        let nodes = pgsql_grammar::parse(source, ParseMode::TypeName, self.options)?;
        let [Node::TypeName(t)] = nodes.as_slice() else {
            return Err(CompileError::InvalidTree(
                "type-name parse mode must return a TypeName",
            ));
        };
        Builtin::resolve(t)
            .map(Builtin::datatype)
            .map_err(|e| e.in_body(source, 0))
    }

    fn validate(&mut self, sql: &str, mode: ParseMode) -> Result<(), CompileError> {
        pgsql_grammar::parse(sql, mode, self.options)?;
        Ok(())
    }
}
