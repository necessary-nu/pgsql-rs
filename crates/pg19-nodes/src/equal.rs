//! PostgreSQL equalfuncs ignores locations and selected backend fields.
use crate::*;

macro_rules! node_types {
    (values { $( $value:ident ($value_id:literal, $value_name:literal, $payload:ty) )* }
     records { $( $record:ident ($id:literal, $name:literal) { $( $field:ident [$cfield:literal]: $ty:ty, )* } )* }) => {
        enum Pair<'a> {
            Nodes(&'a Node, &'a Node),
            $($record(&'a $record, &'a $record),)*
        }
        impl<'a> Pair<'a> {
            fn compare(self, work: &mut Vec<Self>) -> Result<bool, NodeTag> {
                match self {
                    Self::Nodes(left, right) => match (left, right) {
                        (Node::Null, Node::Null) => Ok(true),
                        $((Node::$value(a), Node::$value(b)) => Ok(a.check(b, work)),)*
                        $((Node::$record(a), Node::$record(b)) => a.fields(b, work),)*
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

trait Tagged {
    fn pair<'a>(&'a self, other: &'a Self) -> Pair<'a>;
}
impl Tagged for Node {
    fn pair<'a>(&'a self, other: &'a Self) -> Pair<'a> {
        Pair::Nodes(self, other)
    }
}

trait Field {
    fn check<'a>(&'a self, other: &'a Self, work: &mut Vec<Pair<'a>>) -> bool;
}
macro_rules! scalar {
    ($($name:ty),*) => {$(impl Field for $name {
        fn check<'a>(&'a self, other: &'a Self, _: &mut Vec<Pair<'a>>) -> bool { self == other }
    })*};
}
scalar!(i32, bool, String, Vec<i32>, Vec<u32>, Option<String>);

impl Field for AConst {
    fn check<'a>(&'a self, other: &'a Self, _: &mut Vec<Pair<'a>>) -> bool {
        self.val == other.val
    }
}

impl Field for [Node] {
    fn check<'a>(&'a self, other: &'a Self, work: &mut Vec<Pair<'a>>) -> bool {
        if self.len() != other.len() {
            return false;
        }
        work.extend(self.iter().zip(other).map(|(a, b)| Pair::Nodes(a, b)));
        true
    }
}

fn nullable<'a, T: Tagged>(a: Option<&'a T>, b: Option<&'a T>, work: &mut Vec<Pair<'a>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            if !std::ptr::eq(a, b) {
                work.push(a.pair(b));
            }
            true
        }
        _ => false,
    }
}

impl<T: Tagged> Field for Option<Box<T>> {
    fn check<'a>(&'a self, other: &'a Self, work: &mut Vec<Pair<'a>>) -> bool {
        nullable(self.as_deref(), other.as_deref(), work)
    }
}
impl<T: Tagged + NodeRecord> Field for Option<NodeBox<T>> {
    fn check<'a>(&'a self, other: &'a Self, work: &mut Vec<Pair<'a>>) -> bool {
        nullable(self.as_deref(), other.as_deref(), work)
    }
}

macro_rules! compare_records {
    ($( $record:ident { plain { $($($plain:ident).+),* } edges { $($($edge:ident).+),* } } )*
      @unsupported { $($missing:ident),* }) => {
        $(impl $record {
            fn fields<'a>(&'a self, #[allow(unused_variables)] other: &'a Self, #[allow(unused_variables, clippy::ptr_arg)] work: &mut Vec<Pair<'a>>) -> Result<bool, NodeTag> {
                Ok(true $(&& self.$($plain).+ == other.$($plain).+)* $(&& self.$($edge).+.check(&other.$($edge).+, work))*)
            }
        })*
        $(impl $missing {
            fn fields<'a>(&'a self, _: &'a Self, _: &mut Vec<Pair<'a>>) -> Result<bool, NodeTag> { Err(NodeTag::$missing) }
        })*
    };
}
include!("equal.in");

impl TypeName {
    /// Compare according to PostgreSQL's equalfuncs, including nested typmods.
    /// The error identifies a backend-only tag without an equality implementation.
    pub fn pg_equal(&self, other: &Self) -> Result<bool, NodeTag> {
        let mut work = vec![self.pair(other)];
        while let Some(pair) = work.pop() {
            if !pair.compare(&mut work)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
