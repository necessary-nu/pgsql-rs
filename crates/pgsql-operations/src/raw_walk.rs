//! Borrow typed raw children in PostgreSQL walker order.
use pgsql_nodes::*;

macro_rules! child {
    (list, $visit:expr, $field:expr) => {
        if !$field.is_empty() {
            $visit(RawRef::List(&$field));
        }
    };
    (shared, $visit:expr, $field:expr) => {
        child!(ptr, $visit, $field)
    };
    (ptr, $visit:expr, $field:expr) => {
        if let Some(n) = $field.as_deref() {
            $visit(n.into());
        }
    };
}

macro_rules! mutable_child {
    (list, $field:expr) => {
        if $field.is_empty() {
            None
        } else {
            Some(RawMut::List(&mut $field))
        }
    };
    (ptr, $field:expr) => {
        $field.as_deref_mut().map(RawMut::from)
    };
    (shared, $field:expr) => {
        $field
            .as_mut()
            .map(std::sync::Arc::make_mut)
            .map(RawMut::from)
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

        impl<'a> From<crate::summary::tree::TreeRef<'a>> for RawRef<'a> {
            fn from(node: crate::summary::tree::TreeRef<'a>) -> Self {
                use crate::summary::tree::TreeRef;
                match node {
                    $(TreeRef::$record(value) => Self::$record(value),)*
                    _ => Self::Leaf,
                }
            }
        }
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

        pub(crate) enum RawMut<'a> {
            List(&'a mut [Node]),
            Leaf,
            $($record(&'a mut $record),)*
        }

        $(impl<'a> From<&'a mut $record> for RawMut<'a> {
            fn from(n: &'a mut $record) -> Self { Self::$record(n) }
        })*

        impl<'a> From<&'a mut Node> for RawMut<'a> {
            fn from(n: &'a mut Node) -> Self {
                match n {
                    Node::List(v) => Self::List(v),
                    $(Node::$record(v) => Self::$record(v),)*
                    _ => Self::Leaf,
                }
            }
        }

        impl<'a> RawMut<'a> {
            pub(crate) fn child_at(self, index: usize) -> Option<Self> {
                match self {
                    Self::List(items) => items.get_mut(index).map(Self::from),
                    $(Self::$record(n) => {
                        let _ = &n;
                        #[allow(unused_mut, unused_variables)]
                        let mut next=0;
                        $(if let Some(child)=mutable_child!($kind,n.$field) {
                            if next==index {return Some(child);}
                            next+=1;
                        })*
                        let _ = next;
                        None
                    },)*
                    Self::Leaf => None
                }
            }
        }

        impl<'a> RawRef<'a> {
            pub(crate) fn tag(self) -> Option<NodeTag> {
                match self {
                    Self::Constant(_) => Some(NodeTag::AConst),
                    Self::List(_) => Some(NodeTag::List),
                    Self::Leaf => None,
                    $(Self::$record(_) => Some(NodeTag::$record),)*
                }
            }

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

impl<'a> From<&'a mut Query> for RawMut<'a> {
    fn from(_: &'a mut Query) -> Self {
        Self::Leaf
    }
}
