//! Borrow typed raw children in PostgreSQL walker order.
use pg19_nodes::*;

macro_rules! child {
    (list, $visit:expr, $field:expr) => {
        if !$field.is_empty() {
            $visit(RawRef::List(&$field));
        }
    };
    (ptr, $visit:expr, $field:expr) => {
        if let Some(n) = $field.as_deref() {
            $visit(n.into());
        }
    };
}

macro_rules! raw_records {
    ($( $record:ident { $($kind:ident $field:ident),* } )*) => {
        #[derive(Clone, Copy)]
        pub(crate) enum RawRef<'a> {
            Constant(&'a AConst),
            List(&'a [Node]),
            Leaf,
            $($record(&'a $record),)*
        }

        $(impl<'a> From<&'a $record> for RawRef<'a> {
            fn from(n: &'a $record) -> Self { Self::$record(n) }
        })*

        impl<'a> From<&'a Node> for RawRef<'a> {
            fn from(n: &'a Node) -> Self {
                match n {
                    Node::List(v) => Self::List(v),
                    Node::AConst(v) => Self::Constant(v),
                    $(Node::$record(v) => Self::$record(v),)*
                    _ => Self::Leaf,
                }
            }
        }

        impl<'a> RawRef<'a> {
            pub(crate) fn children(self, visit: &mut impl FnMut(Self)) {
                match self {
                    Self::List(items) => items.iter().for_each(|n| visit(n.into())),
                    $(Self::$record(n) => {
                        let _ = n;
                        $(child!($kind, visit, n.$field);)*
                    },)*
                    Self::Leaf | Self::Constant(_) => {}
                }
            }
        }
    };
}
include!("raw_walk.in");

impl<'a> From<&'a Query> for RawRef<'a> {
    fn from(_: &'a Query) -> Self {
        Self::Leaf
    }
}
