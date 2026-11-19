use std::collections::BTreeSet;
use std::fmt::{self, Display, Write as _};

use pg19_nodes::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    DepthLimit,
    NonFiniteFloat,
}

impl Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::DepthLimit => "raw tree exceeds the JSON nesting limit",
            Self::NonFiniteFloat => "non-finite raw tree field cannot be represented in JSON",
        })
    }
}
impl std::error::Error for EncodeError {}

type Result = std::result::Result<(), EncodeError>;

#[derive(Default)]
struct Writer {
    text: String,
    depth: u16,
}

trait Object {
    fn fields(&self, out: &mut Writer) -> Result;
}

impl Writer {
    fn object(&mut self, fields: impl FnOnce(&mut Self) -> Result) -> Result {
        if self.depth == 128 {
            return Err(EncodeError::DepthLimit);
        }
        self.depth += 1;
        self.text.push('{');
        fields(self)?;
        self.text.push('}');
        self.depth -= 1;
        Ok(())
    }

    fn key(&mut self, key: &str) {
        if !self.text.ends_with('{') {
            self.text.push(',');
        }
        self.text.push('"');
        self.text.push_str(key);
        self.text.push_str("\":");
    }

    fn string(&mut self, text: &str) {
        // libpg_query also escapes angle brackets, including within identifiers.
        let quoted = serde_json::to_string(text).expect("strings serialize to JSON");
        self.text
            .push_str(&quoted.replace('<', "\\u003c").replace('>', "\\u003e"));
    }

    fn number(&mut self, key: &str, value: impl Display) {
        self.key(key);
        write!(self.text, "{value}").unwrap();
    }

    fn write_number<T: Display + Default + PartialEq>(&mut self, key: &str, value: &T) {
        if *value != T::default() {
            self.number(key, value);
        }
    }

    fn write_location(&mut self, key: &str, value: &Location) {
        self.write_number(key, &value.as_pg());
    }

    fn write_char(&mut self, key: &str, value: &u8) {
        if *value != 0 {
            self.key(key);
            self.string(&char::from(*value).to_string());
        }
    }

    fn write_bool(&mut self, key: &str, value: &bool) {
        if *value {
            self.key(key);
            self.text.push_str("true");
        }
    }

    fn write_enum(&mut self, key: &str, name: Option<&str>) {
        self.key(key);
        self.string(name.unwrap_or("(null)"));
    }

    fn write_float(&mut self, key: &str, value: &f64) -> Result {
        if !value.is_finite() {
            return Err(EncodeError::NonFiniteFloat);
        }
        self.key(key);
        write!(self.text, "{value:.6}").unwrap();
        Ok(())
    }

    fn write_string(&mut self, key: &str, value: &Option<String>) {
        if let Some(s) = value.as_ref().filter(|s| !s.is_empty()) {
            self.key(key);
            self.string(s);
        }
    }

    fn write_string_node(&mut self, key: &str, value: &Option<String>) -> Result {
        if let Some(s) = value {
            self.key(key);
            self.object(|w| {
                w.key("sval");
                w.string(s);
                Ok(())
            })?;
        }
        Ok(())
    }

    fn write_pointer<P: std::ops::Deref>(&mut self, key: &str, value: &Option<P>) -> Result
    where
        P::Target: Object + Sized,
    {
        if let Some(node) = value.as_deref() {
            self.write_specific_node(key, node)?;
        }
        Ok(())
    }

    fn write_specific_node(&mut self, key: &str, value: &impl Object) -> Result {
        self.key(key);
        self.object(|w| value.fields(w))
    }

    fn array<T>(
        &mut self,
        values: impl IntoIterator<Item = T>,
        item: impl Fn(&mut Self, T) -> Result,
    ) -> Result {
        self.text.push('[');
        for (index, value) in values.into_iter().enumerate() {
            if index > 0 {
                self.text.push(',');
            }
            item(self, value)?;
        }
        self.text.push(']');
        Ok(())
    }

    fn write_list(&mut self, key: &str, values: &[Node]) -> Result {
        if !values.is_empty() {
            self.key(key);
            self.array(values, |w, n| w.object(|w| n.fields(w)))?;
        }
        Ok(())
    }

    fn numbers<'a, T: Display + 'a>(
        &mut self,
        key: &str,
        numbers: impl IntoIterator<Item = &'a T>,
    ) -> Result {
        self.key(key);
        self.array(numbers, |w, x| {
            write!(w.text, "{x}").unwrap();
            Ok(())
        })
    }

    fn write_bitmapset(&mut self, key: &str, set: &BTreeSet<u32>) -> Result {
        if !set.is_empty() {
            self.numbers(key, set)?;
        }
        Ok(())
    }
}

macro_rules! json_records {
    ($n:ident, $out:ident; $( $name:ident $key:literal { $($fields:expr;)* } )*) => {
        $(impl Object for $name {
            #[allow(unused_variables)]
            fn fields(&self, $out: &mut Writer) -> Result {
                let $n = self;
                $($fields;)*
                Ok(())
            }
        })*
        impl Object for Node {
            fn fields(&self, out: &mut Writer) -> Result {
                match self {
                    $(Self::$name(n) => out.write_specific_node($key, n.as_ref()),)*
                    Self::AConst(n) => out.write_specific_node("A_Const", n),
                    Self::Null | Self::XidList(_) => Ok(()),
                    Self::Integer(v) => scalar(out, "Integer", |w| w.write_number("ival", v)),
                    Self::Float(v) => text_node(out, "Float", "fval", v),
                    Self::String(v) => text_node(out, "String", "sval", v),
                    Self::BitString(v) => text_node(out, "BitString", "bsval", v),
                    Self::Boolean(v) => scalar(out, "Boolean", |w| w.number("boolval", v)),
                    Self::List(v) => { out.key("List"); out.object(|w| {
                        w.key("items"); w.array(v, |w, n| w.object(|w| n.fields(w)))
                    }) }
                    Self::IntList(v) => { out.key("IntList"); out.object(|w| w.numbers("items", v)) }
                    Self::OidList(v) => { out.key("OidList"); out.object(|w| w.numbers("items", v)) }
                }
            }
        }
    };
}

fn scalar(out: &mut Writer, key: &str, fields: impl FnOnce(&mut Writer)) -> Result {
    out.key(key);
    out.object(|w| {
        fields(w);
        Ok(())
    })
}

fn text_node(out: &mut Writer, key: &str, field: &str, text: &str) -> Result {
    scalar(out, key, |w| {
        w.key(field);
        w.string(text)
    })
}

impl Object for AConst {
    fn fields(&self, out: &mut Writer) -> Result {
        match &self.val {
            Literal::Null => out.write_bool("isnull", &true),
            Literal::Integer(v) => scalar(out, "ival", |w| w.write_number("ival", v))?,
            Literal::Float(v) => text_node(out, "fval", "fval", v)?,
            Literal::String(v) => text_node(out, "sval", "sval", v)?,
            Literal::Boolean(v) => scalar(out, "boolval", |w| w.write_bool("boolval", v))?,
            Literal::BitString(v) => text_node(out, "bsval", "bsval", v)?,
        }
        out.number("location", self.location.as_pg());
        Ok(())
    }
}

include!("json-records.in");

// [spec:pgq:req:codec.json-fields]
pub fn node_json(node: &Node) -> std::result::Result<String, EncodeError> {
    if matches!(node, Node::Null) {
        return Ok("null".into());
    }
    let mut w = Writer::default();
    w.object(|w| node.fields(w))?;
    Ok(w.text)
}

// [spec:pgq:req:codec.json-envelope]
pub fn statements_json(stmts: &[RawStmt]) -> std::result::Result<String, EncodeError> {
    let mut w = Writer::default();
    w.object(|w| {
        w.number("version", 190000);
        w.key("stmts");
        w.array(stmts, |w, stmt| w.object(|w| stmt.fields(w)))
    })?;
    Ok(w.text)
}
