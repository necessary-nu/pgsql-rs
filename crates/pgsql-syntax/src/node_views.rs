use crate::*;

macro_rules! node_views {
    ($($variant:ident($record:ident, $name:literal),)*) => {
        #[derive(Debug, Copy, Clone)]
        pub enum NodeRef<'a> { $($variant(&'a protobuf::$record),)* }
        /// A mutable record borrowed for one visitor callback.
        #[derive(Debug)]
        pub enum NodeRefMut<'a> { $($variant(&'a mut protobuf::$record),)* }
        impl NodeRefMut<'_> {
            pub(crate) fn into_raw(self) -> NodeMut {
                match self { $(Self::$variant(n) => NodeMut::$variant(n),)* }
            }
            fn reborrow(&mut self) -> NodeRefMut<'_> {
                match self { $(Self::$variant(n) => NodeRefMut::$variant(n),)* }
            }
        }
        #[derive(Debug, Copy, Clone)]
        pub enum NodeMut { $($variant(*mut protobuf::$record),)* }

        impl NodeEnum {
            pub fn to_ref(&self) -> NodeRef<'_> {
                match self { $(Self::$variant(n) => NodeRef::$variant(n),)* }
            }
            pub fn borrow_mut(&mut self) -> NodeRefMut<'_> {
                match self { $(Self::$variant(n) => NodeRefMut::$variant(n),)* }
            }
            pub fn to_mut(&mut self) -> NodeMut {
                match self { $(Self::$variant(n) => n.to_mut(),)* }
            }
            pub(crate) fn kind_name(&self) -> &'static str {
                self.to_ref().kind_name()
            }
        }
        impl NodeRef<'_> {
            pub(crate) fn kind_name(&self) -> &'static str {
                match self { $(Self::$variant(_) => $name,)* }
            }
            pub fn to_enum(&self) -> NodeEnum {
                match *self { $(Self::$variant(n) => NodeEnum::$variant(n.clone().into()),)* }
            }
        }
        impl NodeMut {
            // The legacy walker only queues pointers derived from the live, exclusively borrowed tree.
            pub(crate) unsafe fn borrow_exclusive<'a>(self) -> NodeRefMut<'a> {
                match self { $(Self::$variant(n) => NodeRefMut::$variant(&mut *n),)* }
            }
            /// # Safety
            /// The pointer must be null or point to a live, initialized node
            /// whose contents can be shared for the duration of this call.
            pub unsafe fn to_enum(&self) -> Result<NodeEnum> {
                Ok(self.as_ref()?.to_enum())
            }
            unsafe fn as_ref(&self) -> Result<NodeRef<'_>> {
                match *self { $(Self::$variant(p) =>
                    Ok(NodeRef::$variant(p.as_ref().ok_or(Error::InvalidPointer)?)),)* }
            }
        }
        $(impl protobuf::$record {
            pub fn to_ref(&self) -> NodeRef<'_> { NodeRef::$variant(self) }
            pub fn to_mut(&mut self) -> NodeMut { NodeMut::$variant(self) }
        })*
    }
}
include!(concat!(env!("OUT_DIR"), "/node_views.rs"));

impl Node {
    pub fn deparse(&self, options: DeparseOptions) -> Result<String> {
        self.node
            .as_ref()
            .ok_or_else(|| Error::Parse("missing statement node".into()))?
            .deparse(options)
    }
}
impl NodeRef<'_> {
    pub fn deparse(&self, options: DeparseOptions) -> Result<String> {
        crate::query::deparse_statements(&[crate::bridge::statement(*self)?], options)
    }
}
impl NodeMut {
    /// # Safety
    /// The pointer must meet the requirements of [NodeMut::to_enum].
    pub unsafe fn deparse(&self, options: DeparseOptions) -> Result<String> {
        self.as_ref()?.deparse(options)
    }
}

include!(concat!(env!("OUT_DIR"), "/visit.rs"));

/// Visit each PG19 record in preorder, including RawStmt wrappers and embedded records.
pub fn visit<'a>(tree: &'a protobuf::ParseResult, mut f: impl FnMut(NodeRef<'a>)) {
    let mut stack: Vec<_> = tree.stmts.iter().rev().map(NodeRef::RawStmt).collect();
    while let Some(node) = stack.pop() {
        f(node);
        let start = stack.len();
        node.children(&mut stack);
        stack[start..].reverse();
    }
}

/// Visit each record with an exclusive borrow. Children are read after the callback,
/// so replacing a field or resizing a list changes the remaining traversal.
///
/// ```compile_fail
/// let mut tree = pgsql_syntax::parse("SELECT 1", 0).unwrap().protobuf;
/// let mut saved = None;
/// pgsql_syntax::visit_mut(&mut tree, |record| saved = Some(record));
/// ```
pub fn visit_mut(tree: &mut protobuf::ParseResult, mut f: impl FnMut(NodeRefMut<'_>)) {
    let mut stack: Vec<_> = tree
        .stmts
        .iter_mut()
        .rev()
        .map(NodeRefMut::RawStmt)
        .collect();
    while let Some(mut node) = stack.pop() {
        f(node.reborrow());
        let start = stack.len();
        node.children(&mut stack);
        stack[start..].reverse();
    }
}
