use std::collections::HashMap;
use std::collections::HashSet;
use std::iter::FromIterator;
use std::string::String;

use crate::*;

impl protobuf::ParseResult {
    pub fn deparse(&self, options: DeparseOptions) -> Result<String> {
        crate::deparse(self, options)
    }

    /// Traverse the nodes used by the upstream query-analysis helpers.
    pub fn nodes(&self) -> Vec<(NodeRef<'_>, i32, Context, bool)> {
        self.stmts
            .iter()
            .filter_map(|s| {
                s.stmt
                    .as_ref()
                    .and_then(|s| s.node.as_ref())
                    .map(|n| n.nodes())
            })
            .flatten()
            .collect()
    }

    /// Returns raw pointers to nested nodes.
    ///
    /// # Safety
    ///
    /// Keep the tree alive and uphold Rust's aliasing rules when dereferencing
    /// a pointer. Replacing a parent or reallocating a list can invalidate
    /// previously returned pointers to its children.
    pub unsafe fn nodes_mut(&mut self) -> Vec<(NodeMut, i32, Context)> {
        self.stmts
            .iter_mut()
            .filter_map(|s| {
                s.stmt
                    .as_mut()
                    .and_then(|s| s.node.as_mut())
                    .map(|n| n.nodes_mut())
            })
            .flatten()
            .collect()
    }
}

/// Result from calling [parse]
#[derive(Debug)]
pub struct ParseResult {
    pub protobuf: protobuf::ParseResult,
    pub warnings: Vec<String>,
    pub tables: Vec<(String, Context)>,
    pub aliases: HashMap<String, String>,
    pub cte_names: Vec<String>,
    pub functions: Vec<(String, Context)>,
    pub filter_columns: Vec<(Option<String>, String)>,
}

impl ParseResult {
    /// Construct a snapshot. For an invalid tree this legacy constructor returns
    /// empty metadata and records the error in warnings. Use [Self::try_new] to propagate it.
    pub fn new(protobuf: protobuf::ParseResult, stderr: String) -> Self {
        let summary = crate::summary::analyze_tree(&protobuf);
        match summary {
            Ok(summary) => Self::from_summary(protobuf, &stderr, summary),
            Err(error) => {
                let empty = Default::default();
                let mut result = Self::from_summary(protobuf, &stderr, empty);
                result.warnings.push(error.to_string());
                result
            }
        }
    }

    pub fn try_new(protobuf: protobuf::ParseResult, stderr: String) -> Result<Self> {
        let summary = crate::summary::analyze_tree(&protobuf)?;
        Ok(Self::from_summary(protobuf, &stderr, summary))
    }

    pub(crate) fn from_summary(
        protobuf: protobuf::ParseResult,
        stderr: &str,
        summary: pgsql_parser::summary::Summary,
    ) -> Self {
        use pgsql_parser::summary::Context as C;
        let context = |c| match C::try_from(c).unwrap_or(C::None) {
            C::Select => Context::Select,
            C::Dml => Context::DML,
            C::Ddl => Context::DDL,
            C::Call => Context::Call,
            C::None => Context::None,
        };
        Self {
            protobuf,
            warnings: stderr
                .lines()
                .filter(|line| line.starts_with("WARNING"))
                .map(|line| line.trim().into())
                .collect(),
            tables: summary
                .tables
                .into_iter()
                .map(|t| (t.name, context(t.context)))
                .collect::<HashSet<_>>()
                .into_iter()
                .collect(),
            aliases: summary.aliases.into_iter().collect(),
            cte_names: summary
                .cte_names
                .into_iter()
                .collect::<HashSet<_>>()
                .into_iter()
                .collect(),
            functions: summary
                .functions
                .into_iter()
                .map(|f| (f.name, context(f.context)))
                .collect::<HashSet<_>>()
                .into_iter()
                .collect(),
            filter_columns: summary
                .filter_columns
                .into_iter()
                .map(|c| ((!c.table_name.is_empty()).then_some(c.table_name), c.column))
                .collect::<HashSet<_>>()
                .into_iter()
                .collect(),
        }
    }

    /// Physical tables at construction. Use [analyze] after editing the tree.
    pub fn tables(&self) -> Vec<String> {
        let mut tables = HashSet::new();
        self.tables.iter().for_each(|(t, _c)| {
            tables.insert(t.to_string());
        });
        Vec::from_iter(tables)
    }

    /// Returns only tables that were selected from
    pub fn select_tables(&self) -> Vec<String> {
        self.tables
            .iter()
            .filter_map(|(table, context)| match context {
                Context::Select => Some(table.to_string()),
                _ => None,
            })
            .collect()
    }

    /// Returns only tables that were modified by the query
    pub fn dml_tables(&self) -> Vec<String> {
        self.tables
            .iter()
            .filter_map(|(table, context)| match context {
                Context::DML => Some(table.to_string()),
                _ => None,
            })
            .collect()
    }

    /// Returns only tables that were modified by DDL statements
    pub fn ddl_tables(&self) -> Vec<String> {
        self.tables
            .iter()
            .filter_map(|(table, context)| match context {
                Context::DDL => Some(table.to_string()),
                _ => None,
            })
            .collect()
    }

    /// Returns all function references
    pub fn functions(&self) -> Vec<String> {
        let mut functions = HashSet::new();
        self.functions.iter().for_each(|(f, _c)| {
            functions.insert(f.to_string());
        });
        Vec::from_iter(functions)
    }

    /// Function names appearing in DDL, including both names in a rename.
    pub fn ddl_functions(&self) -> Vec<String> {
        self.functions
            .iter()
            .filter_map(|(function, context)| match context {
                Context::DDL => Some(function.to_string()),
                _ => None,
            })
            .collect()
    }

    /// Returns functions that were called
    pub fn call_functions(&self) -> Vec<String> {
        self.functions
            .iter()
            .filter_map(|(function, context)| match context {
                Context::Call => Some(function.to_string()),
                _ => None,
            })
            .collect()
    }

    /// Converts the parsed query back into a SQL string
    pub fn deparse(&self, options: DeparseOptions) -> Result<String> {
        crate::deparse(&self.protobuf, options)
    }

    /// Intelligently truncates queries to a max length.
    ///
    /// # Example
    ///
    /// ```rust
    /// let query = "INSERT INTO \"x\" (a, b, c, d, e, f) VALUES ($1)";
    /// let result = pgsql_syntax::parse(query, 0).unwrap();
    /// assert_eq!(result.truncate(32).unwrap(), "INSERT INTO x (...) VALUES (...)")
    /// ```
    pub fn truncate(&self, max_length: usize) -> Result<String> {
        crate::truncate(&self.protobuf, max_length)
    }

    /// Returns all statement types in the query
    pub fn statement_types(&self) -> Vec<&str> {
        self.protobuf
            .stmts
            .iter()
            .filter_map(|s| s.stmt.as_ref()?.node.as_ref().map(NodeEnum::kind_name))
            .filter(|name| name.ends_with("Stmt") || *name == "AlterTableCmd")
            .collect()
    }
}
