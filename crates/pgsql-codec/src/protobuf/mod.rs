//! The PG19 compatibility wire schema is embedded in the library.
//! Proto3 scalar string fields have no presence. Their empty default decodes
//! to a null native string pointer. Message-valued strings and literal oneofs
//! carry presence, including when the contained string is empty.
use pgsql_nodes::*;
use prost_reflect::{
    prost::Message, DescriptorPool, DynamicMessage, FieldDescriptor, MessageDescriptor,
    ReflectMessage, Value,
};
use std::{fmt, sync::LazyLock};

mod fields;
mod records;
mod scan;
mod values;
pub use scan::{scan_from_protobuf, scan_protobuf, ScanToken};

static POOL: LazyLock<DescriptorPool> = LazyLock::new(|| {
    DescriptorPool::decode(&include_bytes!("../../../../protobuf/pg19/pg_query.pb")[..])
        .expect("checked-in PG19 descriptor set")
});

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtobufError {
    DepthLimit,
    Version(i32),
    Unsupported(String),
    Invalid(String),
}

impl fmt::Display for ProtobufError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DepthLimit => write!(f, "PG19 protobuf nesting limit exceeded"),
            Self::Version(v) => write!(f, "unsupported PostgreSQL version {v}"),
            Self::Unsupported(s) => write!(f, "unsupported raw tree field or node {s}"),
            Self::Invalid(s) => write!(f, "invalid PG19 protobuf data: {s}"),
        }
    }
}
impl std::error::Error for ProtobufError {}

type Result<T> = std::result::Result<T, ProtobufError>;

fn schema(name: &str) -> MessageDescriptor {
    POOL.get_message_by_name(&format!("pg_query.{name}"))
        .expect("schema message")
}

fn decode_message(name: &str, bytes: &[u8]) -> Result<DynamicMessage> {
    DynamicMessage::decode(schema(name), bytes).map_err(|e| {
        let diagnostic = e.to_string();
        if diagnostic.ends_with("recursion limit reached") {
            ProtobufError::DepthLimit
        } else {
            ProtobufError::Invalid(diagnostic)
        }
    })
}

fn message(value: Value) -> Result<DynamicMessage> {
    match value {
        Value::Message(m) => Ok(m),
        _ => Err(ProtobufError::Invalid("expected message".into())),
    }
}

fn list(value: Value) -> Result<Vec<Value>> {
    match value {
        Value::List(v) => Ok(v),
        _ => Err(ProtobufError::Invalid("expected repeated field".into())),
    }
}

fn field(m: &DynamicMessage, number: u32) -> Value {
    m.get_field_by_number(number)
        .expect("schema field")
        .into_owned()
}

fn entries(m: &mut DynamicMessage, number: u32) -> Result<Vec<Value>> {
    m.take_field_by_number(number)
        .map(list)
        .unwrap_or(Ok(Vec::new()))
}

#[derive(Copy, Clone)]
struct Depth(u8);

impl Depth {
    fn child(self) -> Result<Self> {
        if self.0 >= 48 {
            Err(ProtobufError::DepthLimit)
        } else {
            Ok(Self(self.0 + 1))
        }
    }
}

trait Record: Sized {
    const NAME: &'static str;
    fn write(&self, m: &mut DynamicMessage, depth: Depth) -> Result<()>;
    fn read(m: DynamicMessage, depth: Depth) -> Result<Self>;

    fn from_message(v: Value, depth: Depth) -> Result<Self> {
        let m = message(v)?;
        if m.descriptor().name() != Self::NAME {
            return Err(ProtobufError::Invalid(format!(
                "expected {}, found {}",
                Self::NAME,
                m.descriptor().full_name()
            )));
        }
        Self::read(m, depth.child()?)
    }
}

trait PackRecord {
    fn to_message(&self, depth: Depth) -> Result<Value>;
}
impl<T: Record> PackRecord for T {
    fn to_message(&self, depth: Depth) -> Result<Value> {
        let mut m = DynamicMessage::new(schema(T::NAME));
        self.write(&mut m, depth.child()?)?;
        Ok(Value::Message(m))
    }
}

fn envelope(name: &str, values: Vec<Value>) -> Vec<u8> {
    let mut m = DynamicMessage::new(schema(name));
    m.set_field_by_number(1, Value::I32(190000));
    m.set_field_by_number(2, Value::List(values));
    m.encode_to_vec()
}

fn decode_envelope(name: &str, bytes: &[u8]) -> Result<Vec<Value>> {
    let mut m = decode_message(name, bytes)?;
    let version = field(&m, 1).as_i32().expect("version is int32");
    if version != 190000 {
        return Err(ProtobufError::Version(version));
    }
    entries(&mut m, 2)
}

// [spec:pgq:req:codec.protobuf-schema]
pub fn statements_protobuf(stmts: &[RawStmt]) -> Result<Vec<u8>> {
    let values = stmts
        .iter()
        .map(|stmt| stmt.to_message(Depth(0)))
        .collect::<Result<_>>()?;
    Ok(envelope("ParseResult", values))
}

pub fn statements_from_protobuf(bytes: &[u8]) -> Result<Vec<RawStmt>> {
    decode_envelope("ParseResult", bytes)?
        .into_iter()
        .map(|v| RawStmt::from_message(v, Depth(0)))
        .collect()
}

pub fn node_protobuf(node: &Node) -> Result<Vec<u8>> {
    Ok(message(node.to_message(Depth(0))?)?.encode_to_vec())
}

pub fn node_from_protobuf(bytes: &[u8]) -> Result<Node> {
    Node::read(decode_message("Node", bytes)?, Depth(1))
}

#[derive(Debug, PartialEq, Eq)]
pub enum SerializationError<E> {
    Parse(E),
    Conversion(ProtobufError),
}

impl<E: fmt::Display> fmt::Display for SerializationError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => e.fmt(f),
            Self::Conversion(e) => e.fmt(f),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for SerializationError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(e) => Some(e),
            Self::Conversion(e) => Some(e),
        }
    }
}

// [spec:pgq:req:codec.errors-and-depth]
pub fn parsed_protobuf<T: AsRef<[RawStmt]>, E>(
    parsed: std::result::Result<T, E>,
) -> std::result::Result<Vec<u8>, SerializationError<E>> {
    let tree = parsed.map_err(SerializationError::Parse)?;
    statements_protobuf(tree.as_ref()).map_err(SerializationError::Conversion)
}
