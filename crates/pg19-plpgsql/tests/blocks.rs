use pg19_plpgsql::{
    compile_block, Block, CompileError, CompiledBody, DataType, Datum, EmbeddedSql, Error,
    Expression, Options, ParseMode, Statement, Variable,
};
use serde_json::{json, Value};

struct Sql;

impl pg19_engine::Actions for Sql {
    type Value = ();
    type Error = std::convert::Infallible;
    fn shift(&mut self, _: pg19_lexer::Token) {}
    fn reduce(
        &mut self,
        _: pg19_tables::Production,
        _: &mut pg19_engine::Reduction<Self::Value>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl EmbeddedSql for Sql {
    fn datatype(&mut self, src: &str) -> Result<DataType, Error> {
        self.validate(src, ParseMode::TypeName)?;
        Ok(match src.trim() {
            "integer" => DataType::Scalar("int4".into()),
            "text" => DataType::Scalar("text".into()),
            "record" => DataType::Record,
            other => panic!("unexpected fixture type {other}"),
        })
    }

    fn validate(&mut self, src: &str, mode: ParseMode) -> Result<(), Error> {
        pg19_engine::parse(src, mode, Options::default(), self).map_err(|error| match error {
            pg19_engine::ParseError::Diagnostic(e) => e,
            pg19_engine::ParseError::Action(never) => match never {},
        })
    }
}

fn compile(source: &str) -> Result<CompiledBody, CompileError> {
    let found = Variable {
        name: "found".into(),
        lineno: 0,
        datatype: DataType::Scalar("bool".into()),
        constant: false,
        not_null: false,
        default: None,
    };
    compile_block(source, Options::default(), None, vec![found], &mut Sql)
}

fn expression(e: &Expression) -> Value {
    let m = match e.mode {
        ParseMode::Sql => 0,
        ParseMode::TypeName => 1,
        ParseMode::PlPgSqlExpr => 2,
        ParseMode::PlPgSqlAssign1 => 3,
        ParseMode::PlPgSqlAssign2 => 4,
        ParseMode::PlPgSqlAssign3 => 5,
    };
    json!({"PLpgSQL_expr":{"query":e.query,"parseMode":m}})
}

fn datum(d: &Datum, id: usize) -> Value {
    let mut v = json!({});
    match d {
        Datum::Variable(var) => {
            v["refname"] = json!(var.name);
            if var.lineno > 0 {
                v["lineno"] = json!(var.lineno);
            }
            if var.constant {
                v["isconst"] = json!(true);
            }
            if var.not_null {
                v["notnull"] = json!(true);
            }
            if let Some(default) = &var.default {
                v["default_val"] = expression(default);
            }
            match &var.datatype {
                DataType::Scalar(name) => {
                    v["datatype"] = json!({"PLpgSQL_type":{"typname":name}});
                    json!({"PLpgSQL_var":v})
                }
                DataType::Record => {
                    if id > 0 {
                        v["dno"] = json!(id);
                    }
                    json!({"PLpgSQL_rec":v})
                }
            }
        }
        Datum::RecordField { name, parent } => {
            v["fieldname"] = json!(name);
            if *parent > 0 {
                v["recparentno"] = json!(parent);
            }
            json!({"PLpgSQL_recfield":v})
        }
    }
}

fn block(b: &Block) -> Value {
    let mut v = json!({"lineno":b.lineno});
    if let Some(label) = &b.label {
        v["label"] = json!(label);
    }
    if !b.body.is_empty() {
        v["body"] = b
            .body
            .iter()
            .map(|s| match s {
                Statement::Block(inner) => block(inner),
                Statement::Assign {
                    lineno,
                    datum,
                    expression: e,
                } => {
                    let mut v = json!({"lineno":lineno, "expr":expression(e)});
                    if *datum > 0 {
                        v["varno"] = json!(datum);
                    }
                    json!({"PLpgSQL_stmt_assign":v})
                }
                Statement::Perform {
                    lineno,
                    expression: e,
                } => json!({"PLpgSQL_stmt_perform":{"lineno":lineno,"expr":expression(e)}}),
            })
            .collect();
    }
    json!({"PLpgSQL_stmt_block":v})
}

fn rows() -> impl Iterator<Item = Value> {
    include_str!("blocks.jsonl")
        .lines()
        .map(|row| serde_json::from_str(row).unwrap())
}

#[test]
fn compile_blocks_against_reference_trees() {
    for row in rows() {
        let src = row["body"].as_str().unwrap();
        let expected = &row["expected"];
        match compile(src) {
            Ok(body) => {
                assert!(expected["error"].is_null(), "{row}");
                let reference = &expected["functions"][0]["PLpgSQL_function"];
                let datums: Vec<_> = body
                    .datums
                    .iter()
                    .enumerate()
                    .map(|(i, d)| datum(d, i))
                    .collect();
                assert_eq!(json!(datums), reference["datums"], "{row}");
                let mut action = reference["action"].clone();
                // The routine compiler adds a RETURN outside the grammar's pl_block.
                let top = &mut action["PLpgSQL_stmt_block"];
                let stmts = top["body"].as_array_mut().unwrap();
                assert_eq!(stmts.pop(), Some(json!({"PLpgSQL_stmt_return":{}})));
                if body.block.label.is_some() {
                    action = stmts.pop().unwrap();
                } else if stmts.is_empty() {
                    top.as_object_mut().unwrap().remove("body");
                }
                assert_eq!(block(&body.block), action, "{row}");
            }
            Err(CompileError::Diagnostic(e)) => {
                let error = &expected["error"];
                assert_eq!(e.sqlstate, error["sqlstate"], "{row}");
                assert_eq!(e.message, error["message"], "{row}");
                assert_eq!(json!(e.position), error["internal_position"], "{row}");
            }
            Err(e) => panic!("{e:?}: {row}"),
        }
    }
}

#[test]
fn trigger_fields_preserve_parent_and_assignment_references() {
    let expected: Value = serde_json::from_str(include_str!(
        "../../../tools/pg19/plpgsql/fixtures/expected.json"
    ))
    .unwrap();
    let reference = &expected["trigger-fields"]["functions"][0]["PLpgSQL_function"];
    let initial: Vec<_> = reference["datums"]
        .as_array()
        .unwrap()
        .iter()
        .take_while(|d| d["PLpgSQL_var"]["lineno"].is_null() && d["PLpgSQL_recfield"].is_null())
        .map(|d| {
            let v = d.get("PLpgSQL_var").unwrap_or(&d["PLpgSQL_rec"]);
            let datatype = v["datatype"]["PLpgSQL_type"]["typname"]
                .as_str()
                .map_or(DataType::Record, |s| DataType::Scalar(s.to_owned()));
            Variable {
                name: v["refname"].as_str().unwrap().to_owned(),
                lineno: 0,
                datatype,
                constant: false,
                not_null: false,
                default: None,
            }
        })
        .collect();
    let prefix = initial.len();
    assert_eq!(prefix, 13);
    let source =
        "\nDECLARE label text := 'ready';\nBEGIN\n  NEW.name = upper(cleanString(OLD.name));\nEND;";
    let body = compile_block(
        source,
        Options::default(),
        Some("changed".to_owned()),
        initial,
        &mut Sql,
    )
    .unwrap();
    let actual = block(&body.block);
    assert_eq!(
        actual["PLpgSQL_stmt_block"]["body"][0],
        reference["action"]["PLpgSQL_stmt_block"]["body"][0]
    );
    let datums: Vec<_> = body
        .datums
        .iter()
        .enumerate()
        .skip(prefix)
        .map(|(id, d)| datum(d, id))
        .collect();
    assert_eq!(datums, reference["datums"].as_array().unwrap()[prefix..]);
}

#[test]
fn unported_actions_are_explicit() {
    let result = compile("BEGIN RAISE NOTICE 'hi'; END;");
    assert_eq!(
        result,
        Err(CompileError::Unsupported {
            production: "stmt_raise: K_RAISE"
        })
    );
}

#[test]
#[ignore = "requires the configured PG19 PL/pgSQL compiler"]
fn verify_live_block_fixtures() {
    let probe = std::path::PathBuf::from(std::env::var_os("PG19_REFERENCE_DIR").unwrap())
        .join("plpgsql-probe");
    let cases: Vec<_> = rows().collect();
    let output = std::process::Command::new(probe)
        .args(
            cases
                .iter()
                .map(|r| format!("DO $wrapped${}$wrapped$", r["body"].as_str().unwrap())),
        )
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.lines().count(), cases.len());
    for (line, row) in stdout.lines().zip(cases) {
        assert_eq!(
            serde_json::from_str::<Value>(line).unwrap(),
            row["expected"],
            "{row}"
        );
    }
}
