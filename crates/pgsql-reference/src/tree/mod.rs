use pgsql_nodes::*;
use std::fmt;
mod scalars;
use scalars::quoted;

#[derive(Debug, PartialEq, Eq)]
pub enum TreeError {
    UnknownEnum(&'static str, i32),
    NonAsciiChar(u8),
}
impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownEnum(name, v) => write!(f, "unknown {name} value {v}"),
            Self::NonAsciiChar(v) => write!(f, "PG char {v} cannot be written as a UTF-8 byte"),
        }
    }
}
impl std::error::Error for TreeError {}

type Result<T> = std::result::Result<T, TreeError>;

trait Emit {
    fn expand<'a>(&'a self, w: &mut Writer<'a>) -> Result<()>;
}

enum Step<'a> {
    Text(&'static str),
    Field(&'static str, &'a dyn Emit),
    StringNode(&'static str, Option<&'a str>),
}

#[derive(Default)]
struct Writer<'a> {
    out: String,
    steps: Vec<Step<'a>>,
}

impl<'a> Writer<'a> {
    fn text(&mut self, s: &'static str) {
        self.steps.push(Step::Text(s));
    }
    fn field(&mut self, prefix: &'static str, value: &'a dyn Emit) {
        self.steps.push(Step::Field(prefix, value));
    }
    fn string_node(&mut self, prefix: &'static str, value: &'a Option<String>) {
        self.steps.push(Step::StringNode(prefix, value.as_deref()));
    }

    fn sequence<T: Emit>(&mut self, items: &'a [T], nil: bool) {
        if nil && items.is_empty() {
            self.out.push_str("<>");
            return;
        }
        self.out.push('(');
        self.text(")");
        for (i, value) in items.iter().enumerate().rev() {
            self.field(if i == 0 { "" } else { " " }, value);
        }
    }

    fn render(mut self) -> Result<String> {
        while let Some(step) = self.steps.pop() {
            match step {
                Step::Text(s) => self.out.push_str(s),
                Step::Field(prefix, value) => {
                    self.out.push_str(prefix);
                    value.expand(&mut self)?;
                }
                Step::StringNode(prefix, s) => {
                    self.out.push_str(prefix);
                    match s {
                        Some(s) => quoted(&mut self.out, s),
                        None => self.out.push_str("<>"),
                    }
                }
            }
        }
        Ok(self.out)
    }
}

trait ChildPointer: std::ops::Deref {}
impl<T> ChildPointer for Box<T> {}
impl ChildPointer for std::sync::Arc<Node> {}
impl<T: NodeRecord> ChildPointer for NodeBox<T> {}

impl<P: ChildPointer> Emit for Option<P>
where
    P::Target: Emit + Sized,
{
    fn expand<'a>(&'a self, w: &mut Writer<'a>) -> Result<()> {
        match self.as_deref() {
            Some(value) => w.field("", value),
            None => w.out.push_str("<>"),
        }
        Ok(())
    }
}
impl Emit for Vec<Node> {
    fn expand<'a>(&'a self, w: &mut Writer<'a>) -> Result<()> {
        w.sequence(self, true);
        Ok(())
    }
}

macro_rules! tree_records {
    ($n:ident, $w:ident; $( $ty:ident $label:literal { $($fields:tt)* } )*) => {
        $(impl Emit for $ty {
            #[allow(unused_variables)]
            fn expand<'a>(&'a self, $w: &mut Writer<'a>) -> Result<()> {
                let $n = self;
                $w.out.push_str(concat!("{",$label));
                $w.text("}");
                let start = $w.steps.len();
                $($fields)*
                $w.steps[start..].reverse();
                Ok(())
            }
        })*
        impl Emit for Node {
            fn expand<'a>(&'a self, w: &mut Writer<'a>) -> Result<()> {
                let record: &dyn Emit = match self {
                    $(Self::$ty(n) => n.as_ref(),)*
                    Self::AConst(n) => n,
                    _ => return scalars::node(self, w),
                };
                w.field("", record);
                Ok(())
            }
        }
    };
}

include!("records.in");

impl Emit for AConst {
    fn expand<'a>(&'a self, w: &mut Writer<'a>) -> Result<()> {
        w.out.push_str("{A_CONST");
        match &self.val {
            Literal::Null => w.out.push_str(" NULL"),
            v => {
                w.out.push_str(" :val ");
                scalars::literal(v, &mut w.out);
            }
        }
        w.text("}");
        w.field(" :location ", &self.location);
        Ok(())
    }
}

pub fn node_text(node: &Node) -> Result<String> {
    let mut w = Writer::default();
    w.field("", node);
    w.render()
}

pub fn nodes_text(nodes: &[Node]) -> Result<String> {
    let mut w = Writer::default();
    w.sequence(nodes, true);
    w.render()
}

pub fn statements_text(stmts: &[RawStmt]) -> Result<String> {
    let mut w = Writer::default();
    w.sequence(stmts, true);
    w.render()
}
