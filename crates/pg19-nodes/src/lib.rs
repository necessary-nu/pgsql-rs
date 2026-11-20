//! Owned raw SQL nodes from the patched PostgreSQL 19.0 grammar.
//!
//! Destruction switches to a heap worklist at a bounded nesting depth, including
//! when parser error cleanup discards an unfinished expression. Build a tagged record with
//! `Node::from(record)` or `Node::from(boxed_record)`. A record variant holds
//! a [NodeBox]; use `owner.into_inner()` to move its fields out. The recursive
//! `SelectStmt.larg` and `rarg` fields also take `Some(record.into())`.
//! Build a nested list with `Node::from(vec![child])` or
//! `Node::List(vec![child].into())`. A [NodeList] supports Vec access,
//! iteration, and `into_vec()`. Generated record structs remain freely movable.
//! Multi-column assignments share one immutable source through an `Arc<Node>`.
//! Cloning also uses a heap worklist. Comparison and debug formatting can recurse.
#![forbid(unsafe_code)]

use std::{boxed::Box, sync::Arc, vec::Vec};

pub type Bitmapset = std::collections::BTreeSet<u32>;

mod copy;
mod equal {
    include!("equal.rs");
    impl<T: Tagged + NodeRecord> Field for Option<NodeBox<T>> {
        fn check<'a>(&'a self, other: &'a Self, work: &mut Work<'a>) -> bool {
            nullable(self.as_deref(), other.as_deref(), work)
        }
    }
}

mod owned;
pub use owned::{NodeBox, NodeList, NodeRecord};

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

        #[derive(Debug, PartialEq)]
        pub enum Node {
            /// NULL list cell, e.g. the first DISTINCT marker in SelectStmt.
            Null,
            $( $value($payload), )*
            $( $record(NodeBox<$record>), )*
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
        impl Node {
            fn detach(self, pending: &mut Vec<Node>) {
                match self {
                    Self::List(mut items) => pending.append(&mut items),
                    $( Self::$record(owner) => owned::Edges::detach(&mut *owner.into_box(), pending), )*
                    _ => {}
                }
            }
        }
        $(
            #[derive(Debug, PartialEq, Default)]
            pub struct $record { $( pub $field: $ty, )* }

            impl From<Box<$record>> for Node {
                fn from(value: Box<$record>) -> Self {
                    Self::$record(NodeBox::from(value))
                }
            }

            impl From<NodeBox<$record>> for Node {
                fn from(value: NodeBox<$record>) -> Self { Self::$record(value) }
            }

            impl From<$record> for Node {
                fn from(value: $record) -> Self {
                    Self::$record(NodeBox::from(value))
                }
            }
        )*
    };
}

include!("nodes.in");

mod statements;
pub use statements::{StatementList, StatementSpanError};
