// PostgreSQL equalfuncs ignores locations and selected backend fields.
use super::*;
use std::collections::HashSet;

macro_rules! node_types {
    (values { $( $value:ident ($value_id:literal, $value_name:literal, $payload:ty) )* }
     records { $( $record:ident ($id:literal, $name:literal) { $( $field:ident [$cfield:literal]: $ty:ty, )* } )* }) => {
        enum Pair<'a> {
            Nodes(&'a Node, &'a Node),
            $($record(&'a $record, &'a $record),)*
        }
        impl<'a> Pair<'a> {
            fn compare(self, work: &mut Work<'a>) -> Result<bool, NodeTag> {
                match self {
                    Self::Nodes(left, right) => match (left, right) {
                        (Node::Null, Node::Null) => Ok(true),
                        $((Node::$value(a), Node::$value(b)) => Ok(a.check(b, work)),)*
                        $((Node::$record(a), Node::$record(b)) => a.resolve(work.context).fields(b.resolve(work.context), work),)*
                        _ => Ok(false),
                    },
                    $(Self::$record(a, b) => a.fields(b, work),)*
                }
            }
        }
        $(impl Tagged for $record {
            fn pair<'a>(&'a self, other: &'a Self) -> Pair<'a> { Pair::$record(self, other) }
        })*
    };
}
include!("nodes.in");

struct Work<'a> {
    context: &'a Context,
    pending: std::vec::Vec<Pair<'a>>,
    shared: HashSet<(usize, usize)>,
}

trait Tagged {
    fn pair<'a>(&'a self, other: &'a Self) -> Pair<'a>;
}
impl Tagged for Node {
    fn pair<'a>(&'a self, other: &'a Self) -> Pair<'a> {
        Pair::Nodes(self, other)
    }
}

trait Field {
    fn check<'a>(&'a self, other: &'a Self, work: &mut Work<'a>) -> bool;
}
macro_rules! scalar {
    ($($name:ty),*) => {$(impl Field for $name {
        fn check<'a>(&'a self, other: &'a Self, _: &mut Work<'a>) -> bool { self == other }
    })*};
}
scalar!(i32, bool, String, Vec<i32>, Vec<u32>, Option<String>);

impl Field for AConst {
    fn check<'a>(&'a self, other: &'a Self, _: &mut Work<'a>) -> bool {
        self.val == other.val
    }
}

impl Field for [Node] {
    fn check<'a>(&'a self, other: &'a Self, work: &mut Work<'a>) -> bool {
        if self.len() != other.len() {
            return false;
        }
        work.pending
            .extend(self.iter().zip(other).map(|(a, b)| Pair::Nodes(a, b)));
        true
    }
}

fn nullable<'a, T: Tagged>(a: Option<&'a T>, b: Option<&'a T>, work: &mut Work<'a>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            if !std::ptr::eq(a, b) {
                work.pending.push(a.pair(b));
            }
            true
        }
        _ => false,
    }
}

macro_rules! compare_records {
    ($( $record:ident { plain { $($($plain:ident).+),* } edges { $($($edge:ident).+),* } } )*
      @unsupported { $($missing:ident),* }) => {
        $(impl $record {
            fn fields<'a>(&'a self, #[allow(unused_variables)] other: &'a Self, #[allow(unused_variables)] work: &mut Work<'a>) -> Result<bool, NodeTag> {
                Ok(true $(&& self.$($plain).+ == other.$($plain).+)* $(&& self.$($edge).+.check(&other.$($edge).+, work))*)
            }
        })*
        $(impl $missing {
            fn fields<'a>(&'a self, _: &'a Self, _: &mut Work<'a>) -> Result<bool, NodeTag> { Err(NodeTag::$missing) }
        })*
    };
}
include!("equal.in");

fn compare<'a>(
    left: &'a TypeName,
    right: &'a TypeName,
    context: &'a Context,
) -> Result<bool, NodeTag> {
    let mut work = Work {
        context,
        pending: std::vec::Vec::new(),
        shared: HashSet::new(),
    };
    work.pending.push(left.pair(right));
    while let Some(pair) = work.pending.pop() {
        if !pair.compare(&mut work)? {
            return Ok(false);
        }
    }
    Ok(true)
}
