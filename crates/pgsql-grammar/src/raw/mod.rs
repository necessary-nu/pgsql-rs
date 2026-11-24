//! The reduction model instantiates the same PostgreSQL schema as the owned API.
pub(crate) use pgsql_nodes::*;
#[allow(unsafe_code)]
mod storage;
pub(crate) use storage::{Arc, Box, Region, String};
mod list;
pub(crate) use list::Vec;
pub(crate) type NodeBox<T> = Box<T>;
pub(crate) type NodeList = Vec<Node>;
pub(crate) type Bitmapset = Vec<u32>;

#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) enum Literal {
    #[default]
    Null,
    Integer(i32),
    Float(String),
    Boolean(bool),
    String(String),
    BitString(String),
}
#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct AConst {
    pub val: Literal,
    pub location: Location,
}
impl From<AConst> for Node {
    fn from(n: AConst) -> Self {
        Self::AConst(n)
    }
}
impl From<Vec<Node>> for Node {
    fn from(n: Vec<Node>) -> Self {
        Self::List(n)
    }
}

macro_rules! node_types {
    (values { $( $value:ident ($value_id:literal, $value_name:literal, $payload:ty) )* }
     records { $( $record:ident ($id:literal, $name:literal) { $( $field:ident [$cfield:literal]: $ty:ty, )* } )* }
    ) => {
        #[allow(dead_code)] // The schema includes backend-only variants.
        #[derive(Debug, PartialEq)]
        pub(crate) enum Node {
            Null,
            $( $value($payload), )*
            $( $record(Box<$record>), )*
        }
        impl Node {
            pub(crate) fn tag(&self) -> Option<NodeTag> {
                match self {
                    Self::Null => None,
                    $(Self::$value(_) => Some(NodeTag::$value),)*
                    $(Self::$record(_) => Some(NodeTag::$record),)*
                }
            }
        }
        impl convert::Shape for Node {
            fn children<'a>(&'a self, work: &mut std::vec::Vec<convert::Visit<'a>>) {
                match self {
                    Self::Null => {},
                    $(Self::$value(value) => convert::Field::queue(value, work),)*
                    $(Self::$record(value) => value.children(work),)*
                }
            }
            fn assemble(&self, ready: &mut convert::Ready) -> pgsql_nodes::Node {
                match self {
                    Self::Null => pgsql_nodes::Node::Null,
                    $(Self::$value(value) => pgsql_nodes::Node::$value(convert::Build::build(value, ready)),)*
                    $(Self::$record(value) => value.assemble(ready),)*
                }
            }
        }
        $(
            #[derive(Debug, PartialEq, Default)]
            pub(crate) struct $record { $(pub $field: $ty,)* }
            impl From<Box<$record>> for Node {
                fn from(record: Box<$record>) -> Self { Self::$record(record) }
            }
            impl From<$record> for Node {
                fn from(record: $record) -> Self { Self::$record(Box::new(record)) }
            }
        )*
    }
}
include!("../../../pgsql-nodes/src/nodes.in");
#[path = "../../../pgsql-nodes/src/statements.rs"]
mod statements;
pub(crate) use statements::StatementList;
mod convert;
pub(crate) use convert::{materialize, statement as owned_statement};

#[path = "../../../pgsql-nodes/src/copy.rs"]
mod copy;

#[path = "../../../pgsql-nodes/src/equal.rs"]
mod equal;
#[path = "../../../pgsql-codec/src/json.rs"]
pub(crate) mod json;
