//! Every owned raw-tree edge, including records embedded directly in other records.
use pgsql_nodes::*;
use std::ops::Deref;

macro_rules! record_edges {
    ($($record:ident { $($child:ident),* } { $($scalar:ident),* })*) => {
        #[derive(Clone, Copy)]
        pub(crate) enum TreeRef<'a> {
            List(&'a [Node]),
            Value,
            $($record(&'a $record),)*
        }
        $(impl<'a> From<&'a $record> for TreeRef<'a> {
            fn from(record: &'a $record) -> Self { Self::$record(record) }
        })*
        impl<'a> From<&'a Node> for TreeRef<'a> {
            fn from(node: &'a Node) -> Self {
                match node {
                    Node::List(items) => Self::List(items),
                    $(Node::$record(record) => Self::$record(record),)*
                    _ => Self::Value,
                }
            }
        }
        impl<'a> TreeRef<'a> {
            pub(super) fn statement_name(self) -> Option<&'static str> {
                let tag = match self {
                    $(Self::$record(_) => NodeTag::$record,)*
                    Self::Value | Self::List(_) => return None,
                };
                let name = tag.pg_name();
                (tag != NodeTag::RawStmt && name.ends_with("Stmt")).then_some(name)
            }

            pub(super) fn children(self, mut visit: impl FnMut(&'static str, Self)) {
                match self {
                    Self::List(nodes) => nodes.iter().for_each(|node| visit("items", node.into())),
                    Self::Value => {}
                    $(Self::$record(record) => {
                        let _ = record;
                        $(if let Some(child) = record.$child.edge() { visit(stringify!($child), child); })*
                    })*
                }
            }
        }
    };
}
include!("../../../pgsql-nodes/src/edges.in");

trait Edge {
    fn edge(&self) -> Option<TreeRef<'_>>;
}
impl Edge for Vec<Node> {
    fn edge(&self) -> Option<TreeRef<'_>> {
        (!self.is_empty()).then_some(TreeRef::List(self))
    }
}
impl<P: Deref> Edge for Option<P>
where
    for<'a> &'a P::Target: Into<TreeRef<'a>>,
{
    fn edge(&self) -> Option<TreeRef<'_>> {
        self.as_deref().map(Into::into)
    }
}
impl Edge for CreateStmt {
    fn edge(&self) -> Option<TreeRef<'_>> {
        Some(self.into())
    }
}
