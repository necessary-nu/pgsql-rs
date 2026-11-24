use pgsql_grammar::{Error, Options, ParseMode};
use pgsql_operations::summary::{
    analyze_statements, summarize_with_limit, summary_protobuf_with_limit, Context, Summary,
    SummaryError,
};
use prost::Message;
use serde_json::Value;

// [spec:pgq:req:summary.truncation/test]
// [spec:pgq:req:summary.protobuf/test]
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
        let limit = case["limit"].as_i64().unwrap_or(-1) as i32;
        let options = Options {
            disable_backslash_quote: case["flags"] == 16,
        };
        let native = summarize_with_limit(sql, mode, options, limit);
        let wire = summary_protobuf_with_limit(sql, mode, options, limit);
        let expected = if let Some(hex) = case["protobuf"].as_str() {
            let bytes: Vec<u8> = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect();
            let mut decoded = Summary::decode(bytes.as_slice()).unwrap();
            if let Some(text) = case["truncated"].as_str() {
                // SummaryResult uses a proto3 scalar and elides the empty string.
                assert_eq!(decoded.truncated_query.as_deref().unwrap_or(""), text);
                decoded.truncated_query = Some(text.to_owned());
            }
            Ok(decoded)
        } else {
            let error = &case["error"];
            assert_eq!(wire.as_ref().unwrap_err(), native.as_ref().unwrap_err());
            if let Err(SummaryError::Parse(Error::Diagnostic(e))) = &native {
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

#[test]
fn pg_query_truncation_considers_short_values_lists() {
    let sql = "INSERT INTO x (a, b, c, d, e, f) VALUES ($1)";
    let stmts = pgsql_grammar::parse_statements(sql, Options::default()).unwrap();
    assert_eq!(
        pgsql_operations::summary::truncate_statements(stmts, 32).unwrap(),
        "INSERT INTO x (...) VALUES (...)"
    );
    let summary = summarize_with_limit(sql, ParseMode::Sql, Options::default(), 32).unwrap();
    assert_eq!(
        summary.truncated_query.as_deref(),
        Some("INSERT INTO x (...) VALUES ($1)")
    );
}

#[test]
fn scoped_analysis_retains_separate_statement_bindings() {
    let sql = "WITH t AS (SELECT 1) SELECT * FROM t; SELECT * FROM t";
    let stmts = pgsql_grammar::parse_statements(sql, Options::default()).unwrap();
    let report = analyze_statements(&stmts);
    assert_eq!(report.cte_names, ["t"]);
    assert_eq!(report.tables.len(), 1);
    let table = &report.tables[0];
    assert_eq!(table.name, "t");
    assert_eq!(table.context(), Context::Select);
    let wire = summarize_with_limit(sql, ParseMode::Sql, Options::default(), -1).unwrap();
    assert!(wire.tables.is_empty());
}

#[test]
fn scoped_analysis_handles_deep_set_operations() {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let mut sql = "WITH t AS (SELECT * FROM physical) SELECT * FROM t".to_string();
            for _ in 0..4500 {
                sql.push_str(" UNION ALL SELECT 1 FROM t");
            }
            let statements = pgsql_grammar::parse_statements(&sql, Options::default()).unwrap();
            let report = analyze_statements(&statements);
            assert_eq!(report.tables.len(), 1);
            assert_eq!(report.tables[0].name, "physical");
            assert_eq!(report.statement_types, ["SelectStmt"]);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn truncate_deep_set_operations_on_a_small_stack() {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let sql = vec!["SELECT 1"; 4500].join(" UNION ALL ");
            let summary =
                summarize_with_limit(&sql, ParseMode::Sql, Default::default(), 10).unwrap();
            assert_eq!(summary.truncated_query.as_deref(), Some("(((((((..."));
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
#[ignore = "set PG19_CORPUS and PG19_SUMMARY_PROBE to compare every extracted SQL case"]
fn compare_extracted_corpus_to_postgres() {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};
    let corpus = std::fs::File::open(std::env::var_os("PG19_CORPUS").unwrap()).unwrap();
    let probe = std::env::var_os("PG19_SUMMARY_PROBE").unwrap();
    let mut successes = 0;
    let mut diagnostics = 0;
    for row in BufReader::new(corpus).lines() {
        let case: Value = serde_json::from_str(&row.unwrap()).unwrap();
        let id = case["id"].as_str().unwrap();
        let sql = case["sql"].as_str().unwrap();
        let (number, mode) = match case["mode"].as_str().unwrap() {
            "default" => (0, ParseMode::Sql),
            "type-name" => (1, ParseMode::TypeName),
            "plpgsql-expr" => (2, ParseMode::PlPgSqlExpr),
            "plpgsql-assign1" => (3, ParseMode::PlPgSqlAssign1),
            "plpgsql-assign2" => (4, ParseMode::PlPgSqlAssign2),
            "plpgsql-assign3" => (5, ParseMode::PlPgSqlAssign3),
            _ => panic!("unknown parse mode for {id}"),
        };
        let options = Options {
            disable_backslash_quote: case["backslash_quote"] == "off",
        };
        let mut child = Command::new(&probe)
            .arg(number.to_string())
            .args([
                "-1",
                if options.disable_backslash_quote {
                    "16"
                } else {
                    "0"
                },
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(sql.as_bytes())
            .unwrap();
        let c = child.wait_with_output().unwrap();
        let native = summarize_with_limit(sql, mode, options, -1);
        match c.status.code() {
            Some(0) => {
                assert_eq!(
                    Summary::decode(c.stdout.as_slice()).unwrap(),
                    native.unwrap(),
                    "{id}"
                );
                successes += 1;
            }
            Some(1) => {
                let expected: Value = serde_json::from_slice(&c.stderr).unwrap();
                let Err(SummaryError::Parse(Error::Diagnostic(e))) = native else {
                    panic!("{id}: {native:?}")
                };
                assert_eq!(
                    serde_json::json!({"position":e.position,"sqlstate":e.sqlstate,"message":e.message}),
                    expected,
                    "{id}"
                );
                diagnostics += 1;
            }
            status => panic!(
                "{id}: reference exited {status:?}: {}",
                String::from_utf8_lossy(&c.stderr)
            ),
        }
    }
    assert!(successes > 0 && diagnostics > 0);
    eprintln!("{successes} matching PG19 summaries, {diagnostics} matching diagnostics");
}
