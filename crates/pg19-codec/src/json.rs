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

struct Writer {
    text: String,
    depth: u16,
}

#[derive(Clone, Copy)]
struct Key(&'static str);

macro_rules! key {
    ($name:literal) => {
        Key(concat!(",\"", $name, "\":"))
    };
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

    fn key(&mut self, key: Key) {
        let prefix = key.0;
        let start = usize::from(self.text.ends_with('{'));
        self.text.push_str(&prefix[start..]);
    }

    fn string(&mut self, text: &str) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        self.text.push('"');
        let mut start = 0;
        for (i, byte) in text.bytes().enumerate() {
            let escape = match byte {
                b'"' => r#"\""#,
                b'\\' => r"\\",
                8 => r"\b",
                9 => r"\t",
                10 => r"\n",
                12 => r"\f",
                13 => r"\r",
                b'<' => r"\u003c",
                b'>' => r"\u003e",
                0..=31 => "",
                _ => continue,
            };
            self.text.push_str(&text[start..i]);
            if escape.is_empty() {
                self.text.push_str(r"\u00");
                self.text.push(char::from(HEX[usize::from(byte >> 4)]));
                self.text.push(char::from(HEX[usize::from(byte & 15)]));
            } else {
                self.text.push_str(escape);
            }
            start = i + 1;
        }
        self.text.push_str(&text[start..]);
        self.text.push('"');
    }

    fn number(&mut self, key: Key, value: impl itoa::Integer) {
        self.key(key);
        self.text.push_str(itoa::Buffer::new().format(value));
    }

    fn write_number<T: itoa::Integer + Copy + Default + PartialEq>(&mut self, key: Key, value: &T) {
        if *value != T::default() {
            self.number(key, *value);
        }
    }

    fn write_location(&mut self, key: Key, value: &Location) {
        self.write_number(key, &value.as_pg());
    }

    fn write_char(&mut self, key: Key, value: &u8) {
        if *value != 0 {
            self.key(key);
            self.string(char::from(*value).encode_utf8(&mut [0; 4]));
        }
    }

    fn write_bool(&mut self, key: Key, value: &bool) {
        if *value {
            self.key(key);
            self.text.push_str("true");
        }
    }

    fn write_enum(&mut self, key: Key, name: Option<&str>) {
        self.key(key);
        self.text.push('"');
        self.text.push_str(name.unwrap_or("(null)"));
        self.text.push('"');
    }

    fn write_float(&mut self, key: Key, value: &f64) -> Result {
        if !value.is_finite() {
            return Err(EncodeError::NonFiniteFloat);
        }
        self.key(key);
        write!(self.text, "{value:.6}").unwrap();
        Ok(())
    }

    fn write_string(&mut self, key: Key, value: &Option<String>) {
        if let Some(s) = value.as_ref().filter(|s| !s.is_empty()) {
            self.key(key);
            self.string(s);
        }
    }

    fn write_string_node(&mut self, key: Key, value: &Option<String>) -> Result {
        if let Some(s) = value {
            self.key(key);
            self.object(|w| {
                w.key(key!("sval"));
                w.string(s);
                Ok(())
            })?;
        }
        Ok(())
    }

    fn write_pointer<P: std::ops::Deref>(&mut self, key: Key, value: &Option<P>) -> Result
    where
        P::Target: Object + Sized,
    {
        if let Some(node) = value.as_deref() {
            self.write_specific_node(key, node)?;
        }
        Ok(())
    }

    fn write_specific_node(&mut self, key: Key, value: &impl Object) -> Result {
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

    fn write_list(&mut self, key: Key, values: &[Node]) -> Result {
        if !values.is_empty() {
            self.key(key);
            self.array(values, |w, n| w.object(|w| n.fields(w)))?;
        }
        Ok(())
    }

    fn numbers<'a, T: itoa::Integer + Copy + 'a>(
        &mut self,
        key: Key,
        numbers: impl IntoIterator<Item = &'a T>,
    ) -> Result {
        self.key(key);
        self.array(numbers, |w, x| {
            w.text.push_str(itoa::Buffer::new().format(*x));
            Ok(())
        })
    }

    fn write_bitmapset(&mut self, key: Key, set: &BTreeSet<u32>) -> Result {
        if !set.is_empty() {
            self.numbers(key, set)?;
        }
        Ok(())
    }
}

// Generated PostgreSQL field names are ASCII identifiers and need no JSON escaping.
macro_rules! write_fields {
    () => {};
    ($out:ident.$method:ident($field:literal, $value:expr)?; $($rest:tt)*) => {
        $out.$method(key!($field), $value)?;
        write_fields!($($rest)*);
    };
    ($out:ident.$method:ident($field:literal, $value:expr); $($rest:tt)*) => {
        $out.$method(key!($field), $value);
        write_fields!($($rest)*);
    };
}

macro_rules! json_records {
    ($n:ident, $out:ident; $( $name:ident $key:literal { $($fields:tt)* } )*) => {
        $(impl Object for $name {
            #[allow(unused_variables)]
            fn fields(&self, $out: &mut Writer) -> Result {
                let $n = self;
                write_fields!($($fields)*);
                Ok(())
            }
        })*
        impl Object for Node {
            fn fields(&self, out: &mut Writer) -> Result {
                match self {
                    $(Self::$name(n) => out.write_specific_node(key!($key), n.as_ref()),)*
                    Self::AConst(n) => out.write_specific_node(key!("A_Const"), n),
                    Self::Null | Self::XidList(_) => Ok(()),
                    Self::Integer(v) => scalar(out, key!("Integer"), |w| w.write_number(key!("ival"), v)),
                    Self::Float(v) => text_node(out, key!("Float"), key!("fval"), v),
                    Self::String(v) => text_node(out, key!("String"), key!("sval"), v),
                    Self::BitString(v) => text_node(out, key!("BitString"), key!("bsval"), v),
                    Self::Boolean(v) => scalar(out, key!("Boolean"), |w| { w.key(key!("boolval")); w.text.push_str(if *v { "true" } else { "false" }); }),
                    Self::List(v) => { out.key(key!("List")); out.object(|w| {
                        w.key(key!("items")); w.array(v, |w, n| w.object(|w| n.fields(w)))
                    }) }
                    Self::IntList(v) => { out.key(key!("IntList")); out.object(|w| w.numbers(key!("items"), v)) }
                    Self::OidList(v) => { out.key(key!("OidList")); out.object(|w| w.numbers(key!("items"), v)) }
                }
            }
        }
    };
}

fn scalar(out: &mut Writer, key: Key, fields: impl FnOnce(&mut Writer)) -> Result {
    out.key(key);
    out.object(|w| {
        fields(w);
        Ok(())
    })
}

fn text_node(out: &mut Writer, key: Key, field: Key, text: &str) -> Result {
    scalar(out, key, |w| {
        w.key(field);
        w.string(text)
    })
}

impl Object for AConst {
    fn fields(&self, out: &mut Writer) -> Result {
        match &self.val {
            Literal::Null => out.write_bool(key!("isnull"), &true),
            Literal::Integer(v) => scalar(out, key!("ival"), |w| w.write_number(key!("ival"), v))?,
            Literal::Float(v) => text_node(out, key!("fval"), key!("fval"), v)?,
            Literal::String(v) => text_node(out, key!("sval"), key!("sval"), v)?,
            Literal::Boolean(v) => {
                scalar(out, key!("boolval"), |w| w.write_bool(key!("boolval"), v))?
            }
            Literal::BitString(v) => text_node(out, key!("bsval"), key!("bsval"), v)?,
        }
        out.number(key!("location"), self.location.as_pg());
        Ok(())
    }
}

include!("json-records.in");

// [spec:pgq:req:codec.json-fields]
pub fn node_json(node: &Node) -> std::result::Result<String, EncodeError> {
    if matches!(node, Node::Null) {
        return Ok("null".into());
    }
    let mut w = Writer {
        text: String::with_capacity(256),
        depth: 0,
    };
    w.object(|w| node.fields(w))?;
    Ok(w.text)
}

// [spec:pgq:req:codec.json-envelope]
pub fn statements_json(stmts: &[RawStmt]) -> std::result::Result<String, EncodeError> {
    let mut w = Writer {
        text: String::with_capacity(256),
        depth: 0,
    };
    w.object(|w| {
        w.number(key!("version"), 190000);
        w.key(key!("stmts"));
        w.array(stmts, |w, stmt| w.object(|w| stmt.fields(w)))
    })?;
    Ok(w.text)
}
