//! Postorder cloning keeps both generic and statically typed edges off the call stack.
use super::*;
use std::vec::Vec as Work;

pub(crate) enum Visit<'a> {
    Read(&'a dyn Shape),
    Finish(&'a dyn Shape),
}

pub(crate) trait Shape {
    fn children<'a>(&'a self, work: &mut Work<Visit<'a>>);
    fn assemble(&self, ready: &mut Work<Node>) -> Node;
}

fn duplicate(root: &dyn Shape) -> Node {
    let mut work = Work::new();
    root.children(&mut work);
    let mut ready = Work::new();
    while let Some(task) = work.pop() {
        match task {
            Visit::Read(shape) => {
                work.push(Visit::Finish(shape));
                shape.children(&mut work);
            }
            Visit::Finish(shape) => {
                let node = shape.assemble(&mut ready);
                ready.push(node);
            }
        }
    }
    let node = root.assemble(&mut ready);
    debug_assert!(ready.is_empty());
    node
}

macro_rules! node_types {
    (values { $( $value:ident ($($payload:tt)*) )* }
     records { $( $record:ident ($($tag:tt)*) { $($fields:tt)* } )* }) => {
        impl Shape for Node {
            fn children<'a>(&'a self, work: &mut Work<Visit<'a>>) {
                match self {
                    Self::Null => {},
                    $(Self::$value(value) => Child::queue(value, work),)*
                    $(Self::$record(record) => record.as_ref().children(work),)*
                }
            }
            fn assemble(&self, ready: &mut Work<Node>) -> Node {
                match self {
                    Self::Null => Self::Null,
                    $(Self::$value(value) => Self::$value(Child::rebuild(value, ready)),)*
                    $(Self::$record(record) => record.as_ref().assemble(ready),)*
                }
            }
        }
    };
}
include!("nodes.in");

impl Clone for Node {
    fn clone(&self) -> Self {
        duplicate(self)
    }
}

// Children are queued in field order, visited last to first, and popped in field order.
pub(crate) trait Child: Sized {
    fn queue<'a>(&'a self, work: &mut Work<Visit<'a>>);
    fn rebuild(&self, ready: &mut Work<Node>) -> Self;
}

impl Child for Vec<Node> {
    fn queue<'a>(&'a self, work: &mut Work<Visit<'a>>) {
        work.extend(self.iter().map(|node| Visit::Read(node)));
    }
    fn rebuild(&self, ready: &mut Work<Node>) -> Self {
        let start = ready.len() - self.len();
        ready.drain(start..).rev().collect()
    }
}

pub(crate) trait Unbox: Shape + Sized {
    fn unbox(node: Node) -> Box<Self>;
}

impl Unbox for Node {
    fn unbox(node: Node) -> Box<Self> {
        Box::new(node)
    }
}

impl<T: Unbox> Child for Option<Box<T>> {
    fn queue<'a>(&'a self, work: &mut Work<Visit<'a>>) {
        if let Some(value) = self.as_deref() {
            work.push(Visit::Read(value));
        }
    }
    fn rebuild(&self, ready: &mut Work<Node>) -> Self {
        self.as_ref()
            .map(|_| T::unbox(ready.pop().expect("boxed child")))
    }
}

macro_rules! scalar {
    ($($t:ty),*) => {$(
        impl Child for $t {
            fn queue<'a>(&'a self, _: &mut Work<Visit<'a>>) {}
            fn rebuild(&self, _: &mut Work<Node>) -> Self { self.clone() }
        }
    )*}
}
// Preserve the shared source when duplicating a multi-column assignment.
scalar!(
    AConst,
    i32,
    String,
    bool,
    Vec<i32>,
    Vec<u32>,
    Option<Arc<Node>>
);

macro_rules! record_copy {
    ($record:ident { $($child:ident),* } { $($plain:ident),* }) => {
        impl Shape for $record {
            fn children<'a>(&'a self, #[allow(unused_variables)] work: &mut Work<Visit<'a>>) {
                $(self.$child.queue(work);)*
            }
            fn assemble(&self, #[allow(unused_variables)] ready: &mut Work<Node>) -> Node {
                Self { $($child: self.$child.rebuild(ready),)* $($plain: self.$plain.clone(),)* }.into()
            }
        }
        impl $record {
            fn copied_record(node: Node) -> Self {
                let Node::$record(owner) = node else { unreachable!("record type follows its source") };
                owner.into_inner()
            }
        }
        impl Unbox for $record {
            fn unbox(node: Node) -> Box<Self> {
                let Node::$record(owner) = node else { unreachable!("child type follows its source field") };
                owner.into_box()
            }
        }
        impl Child for $record {
            fn queue<'a>(&'a self, work: &mut Work<Visit<'a>>) { work.push(Visit::Read(self)); }
            fn rebuild(&self, ready: &mut Work<Node>) -> Self { Self::copied_record(ready.pop().expect("embedded child")) }
        }
        impl Clone for $record {
            fn clone(&self) -> Self { Self::copied_record(duplicate(self)) }
        }
    };
}
macro_rules! record_edges {
    ($($record:ident { $($child:ident),* } { $($plain:ident),* })*) => {$(
        record_copy!($record { $($child),* } { $($plain),* });
    )*};
}
include!("edges.in");
