use prost::{Enumeration, Message};
use std::collections::BTreeMap;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
pub enum Context {
    None = 0,
    Select = 1,
    Dml = 2,
    Ddl = 3,
    Call = 4,
}

#[derive(Clone, PartialEq, Eq, Message)]
pub struct Table {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(string, tag = "2")]
    pub schema_name: String,
    #[prost(string, tag = "3")]
    pub table_name: String,
    #[prost(enumeration = "Context", tag = "4")]
    pub context: i32,
}

#[derive(Clone, PartialEq, Eq, Message)]
pub struct Function {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(string, tag = "2")]
    pub function_name: String,
    #[prost(string, tag = "3")]
    pub schema_name: String,
    #[prost(enumeration = "Context", tag = "4")]
    pub context: i32,
}

#[derive(Clone, PartialEq, Eq, Message)]
pub struct FilterColumn {
    #[prost(string, tag = "1")]
    pub schema_name: String,
    #[prost(string, tag = "2")]
    pub table_name: String,
    #[prost(string, tag = "3")]
    pub column: String,
}

#[derive(Clone, PartialEq, Eq, Message)]
pub struct Summary {
    #[prost(message, repeated, tag = "1")]
    pub tables: Vec<Table>,
    #[prost(btree_map = "string,string", tag = "2")]
    pub aliases: BTreeMap<String, String>,
    #[prost(string, repeated, tag = "3")]
    pub cte_names: Vec<String>,
    #[prost(message, repeated, tag = "4")]
    pub functions: Vec<Function>,
    #[prost(message, repeated, tag = "5")]
    pub filter_columns: Vec<FilterColumn>,
    #[prost(string, repeated, tag = "6")]
    pub statement_types: Vec<String>,
    #[prost(string, optional, tag = "7")]
    pub truncated_query: Option<String>,
}
