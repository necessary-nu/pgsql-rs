//! Owned raw SQL nodes from the patched PostgreSQL 19.0 grammar.
#![forbid(unsafe_code)]

mod location;
pub use location::Location;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Literal {
    #[default]
    Null,
    Integer(i32),
    Float(String),
    Boolean(bool),
    String(String),
    BitString(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AConst {
    pub val: Literal,
    pub location: Location,
}

impl From<AConst> for Node {
    fn from(value: AConst) -> Self {
        Self::AConst(value)
    }
}

// Some C enum domains start at one or use character codes. Unset fields in
// a freshly allocated node still hold zero.
macro_rules! pg_enums {
    ($( $name:ident { $( $constant:ident = $value:expr, )* } )*) => {$(
        #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(pub i32);
        #[allow(non_upper_case_globals)] // Preserve upstream spellings used by grammar actions.
        impl $name {
            $( pub const $constant: Self = Self($value); )*
            pub fn pg_name(self) -> Option<&'static str> {
                $( if self == Self::$constant { return Some(stringify!($constant)); } )*
                None
            }
        }
    )*};
}

include!("enums.in");

// [spec:pgq:req:tree.node-layout]
macro_rules! node_types {
    (values { $( $value:ident ($value_id:literal, $value_name:literal, $payload:ty) )* }
     records { $( $record:ident ($id:literal, $name:literal) { $( $field:ident [$cfield:literal]: $ty:ty, )* } )* }
    ) => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
        #[repr(u16)]
        pub enum NodeTag {
            $( $value = $value_id, )*
            $( $record = $id, )*
        }

        impl NodeTag {
            pub fn pg_name(self) -> &'static str {
                match self {
                    $( Self::$value => $value_name, )*
                    $( Self::$record => $name, )*
                }
            }

            /// Fields in C declaration order, excluding the common tag header.
            /// Constants, scalar values and lists use their dedicated payloads.
            pub fn record_fields(self) -> Option<&'static [&'static str]> {
                match self {
                    $( Self::$record => Some(&[$($cfield),*]), )*
                    _ => None,
                }
            }
        }

        #[derive(Debug, Clone, PartialEq)]
        pub enum Node {
            /// NULL list cell, e.g. the first DISTINCT marker in SelectStmt.
            Null,
            $( $value($payload), )*
            $( $record(Box<$record>), )*
        }
        impl Node {
            pub fn tag(&self) -> Option<NodeTag> {
                match self {
                    Self::Null => None,
                    $( Self::$value(_) => Some(NodeTag::$value), )*
                    $( Self::$record(_) => Some(NodeTag::$record), )*
                }
            }
        }
        $(
            #[derive(Debug, Clone, PartialEq, Default)]
            pub struct $record { $( pub $field: $ty, )* }

            impl From<$record> for Node {
                fn from(value: $record) -> Self {
                    Self::$record(Box::new(value))
                }
            }
        )*
    };
}

include!("nodes.in");

mod statements;
pub use statements::{StatementList, StatementSpanError};
