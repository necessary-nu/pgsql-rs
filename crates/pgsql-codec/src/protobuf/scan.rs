use super::*;
use pgsql_lexer::{KeywordKind, Kind, Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanToken {
    pub start: usize,
    pub end: usize,
    pub kind: Kind,
    pub keyword: Option<KeywordKind>,
}

fn keyword_number(keyword: Option<KeywordKind>) -> i32 {
    match keyword {
        None => 0,
        Some(KeywordKind::Unreserved) => 1,
        Some(KeywordKind::ColumnName) => 2,
        Some(KeywordKind::TypeFunctionName) => 3,
        Some(KeywordKind::Reserved) => 4,
    }
}

fn scan_value(t: &Token) -> Result<Value> {
    if t.end < t.start {
        return Err(ProtobufError::Invalid("inverted token span".into()));
    }
    let mut m = DynamicMessage::new(schema("ScanToken"));
    for (field, offset) in [(1, t.start), (2, t.end)] {
        let i = i32::try_from(offset)
            .map_err(|_| ProtobufError::Invalid("token position exceeds int32".into()))?;
        m.set_field_by_number(field, Value::I32(i));
    }
    m.set_field_by_number(4, Value::EnumNumber(t.kind.id().into()));
    m.set_field_by_number(5, Value::EnumNumber(keyword_number(t.keyword)));
    Ok(Value::Message(m))
}

// [spec:pgq:req:lex.token-results]
pub fn scan_protobuf(tokens: &[Token]) -> Result<Vec<u8>> {
    Ok(envelope(
        "ScanResult",
        tokens.iter().map(scan_value).collect::<Result<_>>()?,
    ))
}

fn scan_token(v: Value) -> Result<ScanToken> {
    let m = message(v)?;
    let offset = |n| {
        usize::try_from(field(&m, n).as_i32().unwrap())
            .map_err(|_| ProtobufError::Invalid("negative token offset".into()))
    };
    let start = offset(1)?;
    let end = offset(2)?;
    if start > end {
        return Err(ProtobufError::Invalid("inverted token span".into()));
    }
    let kind = u16::try_from(field(&m, 4).as_enum_number().unwrap())
        .ok()
        .and_then(Kind::from_id)
        .ok_or_else(|| ProtobufError::Invalid("unknown scan token".into()))?;
    let keyword = match field(&m, 5).as_enum_number().unwrap() {
        0 => None,
        1 => Some(KeywordKind::Unreserved),
        2 => Some(KeywordKind::ColumnName),
        3 => Some(KeywordKind::TypeFunctionName),
        4 => Some(KeywordKind::Reserved),
        _ => return Err(ProtobufError::Invalid("unknown keyword category".into())),
    };
    Ok(ScanToken {
        start,
        end,
        kind,
        keyword,
    })
}

pub fn scan_from_protobuf(bytes: &[u8]) -> Result<Vec<ScanToken>> {
    decode_envelope("ScanResult", bytes)?
        .into_iter()
        .map(scan_token)
        .collect()
}
