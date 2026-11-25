//! Materialize an independent tree in postorder, including deeply nested expressions.
use super::*;
use pgsql_nodes as owned;
use std::{boxed::Box as OwnedBox, collections::HashMap, sync::Arc as Shared, vec::Vec as Stack};

pub(crate) enum Visit<'a> {
    Read(&'a dyn Shape),
    Finish(&'a dyn Shape),
    Shared(Arc<Node>),
    Store(usize),
}
pub(crate) trait Shape {
    fn children<'a>(&'a self, arena: &'a Arena, work: &mut Stack<Visit<'a>>);
    fn assemble(&self, ready: &mut Ready) -> owned::Node;
}
pub(crate) struct Ready<'a> {
    pub(super) arena: &'a Arena,
    nodes: Stack<owned::Node>,
    shared: HashMap<usize, Shared<owned::Node>>,
}

pub(crate) fn materialize(root: &impl Shape, arena: &Arena) -> owned::Node {
    let mut work = Stack::new();
    let mut ready = Ready {
        arena,
        nodes: Stack::new(),
        shared: HashMap::new(),
    };
    root.children(arena, &mut work);
    while let Some(task) = work.pop() {
        match task {
            Visit::Read(n) => {
                work.push(Visit::Finish(n));
                n.children(arena, &mut work);
            }
            Visit::Finish(n) => {
                let node = n.assemble(&mut ready);
                ready.nodes.push(node);
            }
            Visit::Shared(n) => {
                if !ready.shared.contains_key(&n.index()) {
                    work.push(Visit::Store(n.index()));
                    work.push(Visit::Read(n.get(arena)));
                }
            }
            Visit::Store(n) => {
                ready
                    .shared
                    .insert(n, Shared::new(ready.nodes.pop().unwrap()));
            }
        }
    }
    root.assemble(&mut ready)
}

pub(crate) fn statement(statement: &RawStmt, arena: &Arena) -> owned::RawStmt {
    let owned::Node::RawStmt(record) = materialize(statement, arena) else {
        unreachable!()
    };
    record.into_inner()
}

pub(super) trait Field {
    fn queue<'a>(&'a self, _: &'a Arena, _: &mut Stack<Visit<'a>>) {}
}
pub(super) trait Build<T> {
    fn build(&self, ready: &mut Ready) -> T;
}
impl<T: Copy> Build<T> for T {
    fn build(&self, _: &mut Ready) -> T {
        *self
    }
}
impl Build<std::string::String> for String {
    fn build(&self, r: &mut Ready) -> std::string::String {
        self.get(r.arena).to_owned()
    }
}
impl Build<Option<std::string::String>> for Option<String> {
    fn build(&self, r: &mut Ready) -> Option<std::string::String> {
        self.as_ref().map(|s| s.get(r.arena).to_owned())
    }
}
impl<T: Stored + Copy + Default> Build<Stack<T>> for Vec<T> {
    fn build(&self, r: &mut Ready) -> Stack<T> {
        self.slice(r.arena).to_vec()
    }
}
impl Build<owned::Bitmapset> for Bitmapset {
    fn build(&self, r: &mut Ready) -> owned::Bitmapset {
        self.slice(r.arena).iter().copied().collect()
    }
}
impl Build<owned::AConst> for AConst {
    fn build(&self, r: &mut Ready) -> owned::AConst {
        let val = match &self.val {
            Literal::Null => owned::Literal::Null,
            Literal::Integer(v) => owned::Literal::Integer(*v),
            Literal::Boolean(v) => owned::Literal::Boolean(*v),
            Literal::Float(v) => owned::Literal::Float(v.build(r)),
            Literal::String(v) => owned::Literal::String(v.build(r)),
            Literal::BitString(v) => owned::Literal::BitString(v.build(r)),
        };
        owned::AConst {
            val,
            location: self.location,
        }
    }
}
impl Field for AConst {}
impl Field for i32 {}
impl Field for bool {}
impl Field for String {}
impl Field for Vec<i32> {}
impl Field for Vec<u32> {}
impl Field for Vec<Node> {
    fn queue<'a>(&'a self, arena: &'a Arena, w: &mut Stack<Visit<'a>>) {
        w.extend(self.slice(arena).iter().map(|n| Visit::Read(n)));
    }
}
impl Build<Stack<owned::Node>> for Vec<Node> {
    fn build(&self, r: &mut Ready) -> Stack<owned::Node> {
        let start = r.nodes.len() - self.len();
        r.nodes.drain(start..).rev().collect()
    }
}
impl Build<owned::NodeList> for Vec<Node> {
    fn build(&self, r: &mut Ready) -> owned::NodeList {
        <Self as Build<Stack<owned::Node>>>::build(self, r).into()
    }
}
pub(super) trait Unbox: Shape + Stored + Copy {
    type Owned;
    fn unbox(node: owned::Node) -> OwnedBox<Self::Owned>;
}
impl Unbox for Node {
    type Owned = owned::Node;
    fn unbox(n: owned::Node) -> OwnedBox<owned::Node> {
        OwnedBox::new(n)
    }
}
impl<T: Unbox> Field for Option<Box<T>> {
    fn queue<'a>(&'a self, arena: &'a Arena, w: &mut Stack<Visit<'a>>) {
        if let Some(n) = self.map(|id| id.get(arena)) {
            w.push(Visit::Read(n));
        }
    }
}
impl<T: Unbox> Build<Option<OwnedBox<T::Owned>>> for Option<Box<T>> {
    fn build(&self, r: &mut Ready) -> Option<OwnedBox<T::Owned>> {
        self.as_ref().map(|_| T::unbox(r.nodes.pop().unwrap()))
    }
}
impl Build<Option<owned::NodeBox<owned::SelectStmt>>> for Option<Box<SelectStmt>> {
    fn build(&self, r: &mut Ready) -> Option<owned::NodeBox<owned::SelectStmt>> {
        self.as_ref()
            .map(|_| SelectStmt::unbox(r.nodes.pop().unwrap()).into())
    }
}
impl Field for Option<Arc<Node>> {
    fn queue<'a>(&'a self, _: &'a Arena, w: &mut Stack<Visit<'a>>) {
        if let Some(n) = *self {
            w.push(Visit::Shared(n));
        }
    }
}
impl Build<Option<Shared<owned::Node>>> for Option<Arc<Node>> {
    fn build(&self, r: &mut Ready) -> Option<Shared<owned::Node>> {
        self.map(|id| r.shared[&id.index()].clone())
    }
}

macro_rules! record_edges {
    ($($record:ident { $($child:ident),* } { $($plain:ident),* })*) => {$(
        impl Shape for $record {
            fn children<'a>(&'a self, #[allow(unused_variables)] arena: &'a Arena, #[allow(unused_variables)] work: &mut Stack<Visit<'a>>) {
                $(self.$child.queue(arena, work);)*
            }
            fn assemble(&self, #[allow(unused_variables)] ready: &mut Ready) -> owned::Node {
                owned::$record {
                    $($child: self.$child.build(ready),)*
                    $($plain: self.$plain.build(ready),)*
                }.into()
            }
        }
        impl Unbox for $record {
            type Owned = owned::$record;
            fn unbox(node: owned::Node) -> OwnedBox<Self::Owned> {
                let owned::Node::$record(record) = node else {
                    unreachable!("typed child changed during materialization")
                };
                record.into_box()
            }
        }
        impl Field for $record {
            fn queue<'a>(&'a self, _: &'a Arena, work: &mut Stack<Visit<'a>>) { work.push(Visit::Read(self)); }
        }
        impl Build<owned::$record> for $record {
            fn build(&self, ready: &mut Ready) -> owned::$record {
                *Self::unbox(ready.nodes.pop().expect("embedded record"))
            }
        }
    )*};
}
include!("../../../pgsql-nodes/src/edges.in");
