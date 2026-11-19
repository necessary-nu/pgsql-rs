use pg19_operations::summary::{summarize, summary_protobuf, Summary};
use pg19_parser::{Error, ParseMode};
use prost::Message;
use serde_json::Value;

// [spec:pgq:req:summary.entities/test]
// [spec:pgq:req:summary.cte-scope/test]
// [spec:pgq:req:summary.nested-filters/test]
#[test]
fn postgres_reference_summary() {
    let modes = [
        ParseMode::Sql,
        ParseMode::TypeName,
        ParseMode::PlPgSqlExpr,
        ParseMode::PlPgSqlAssign1,
        ParseMode::PlPgSqlAssign2,
        ParseMode::PlPgSqlAssign3,
    ];
    let mut mismatches = Vec::new();
    for row in include_str!("summary.jsonl").lines() {
        let case: Value = serde_json::from_str(row).unwrap();
        let sql = case["sql"].as_str().unwrap();
        let mode = modes[case["mode"].as_u64().unwrap() as usize];
        let native = summarize(sql, mode, Default::default());
        let wire = summary_protobuf(sql, mode, Default::default());
        let expected = if let Some(hex) = case["protobuf"].as_str() {
            let bytes: Vec<u8> = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect();
            Ok(Summary::decode(bytes.as_slice()).unwrap())
        } else {
            let error = &case["error"];
            assert_eq!(wire.as_ref().unwrap_err(), native.as_ref().unwrap_err());
            if let Err(Error::Diagnostic(e)) = &native {
                if e.message == error["message"]
                    && e.sqlstate == error["sqlstate"]
                    && e.position == error["position"].as_u64().unwrap() as usize
                {
                    continue;
                }
            }
            mismatches.push(format!("{}: {native:?}, expected {error}", case["id"]));
            continue;
        };
        if expected != native {
            mismatches.push(format!("{}: {native:?}, expected {expected:?}", case["id"]));
        } else {
            assert_eq!(
                Summary::decode(wire.unwrap().as_slice()).unwrap(),
                expected.unwrap(),
                "{}",
                case["id"]
            );
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
