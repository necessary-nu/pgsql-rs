use super::{projected_type, raw};
use pg19_plpgsql::{
    compile_routines, Builtin, CompileError, DataType, EmbeddedSql, NativeSql, Options, ParseMode,
};
use serde_json::Value;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Command,
};

struct SqlReference {
    run: PathBuf,
}

fn query<E: AsRef<OsStr>>(probe: &Path, args: &[E]) -> Value {
    let output = Command::new(probe).args(args).output().unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    serde_json::from_slice(&output.stdout).unwrap()
}

impl EmbeddedSql for SqlReference {
    fn datatype(&mut self, s: &str) -> Result<DataType, CompileError> {
        if s.contains('%') {
            return NativeSql::default().datatype(s);
        }
        let t = query(&self.run.join("plpgsql-probe"), &["--type-name", s]);
        assert!(t.get("message").is_none(), "{t}");
        Ok(Builtin::resolve(&projected_type(&t))?.datatype())
    }
    fn validate(&mut self, s: &str, mode: ParseMode) -> Result<(), CompileError> {
        let mode = match mode {
            ParseMode::Sql => "default",
            ParseMode::TypeName => "type-name",
            ParseMode::PlPgSqlExpr => "plpgsql-expr",
            ParseMode::PlPgSqlAssign1 => "plpgsql-assign1",
            ParseMode::PlPgSqlAssign2 => "plpgsql-assign2",
            ParseMode::PlPgSqlAssign3 => "plpgsql-assign3",
        };
        let result = query(&self.run.join("patched/probe"), &[s, "--mode", mode]);
        assert!(
            result["parse_error"].is_null(),
            "{s}: {}",
            result["parse_error"]
        );
        Ok(())
    }
}

#[test]
#[ignore = "uses PG19 reference SQL trees and types to isolate native PL/pgSQL body actions"]
fn compare_project_sample_routines() {
    let run = PathBuf::from(std::env::var_os("PG19_REFERENCE_DIR").unwrap());
    let sample = include_str!("../../../../tools/pg19/libpg_query/test/plpgsql_samples.sql");
    let expected: Vec<Value> = serde_json::from_str(include_str!(
        "../../../../tools/pg19/libpg_query/test/plpgsql_samples.expected.json"
    ))
    .unwrap();
    let signatures = query(&run.join("plpgsql-probe"), &["--routine-inputs", sample]);
    let signatures = signatures.as_array().unwrap();
    assert_eq!(expected.len(), signatures.len());
    let mut sql = SqlReference { run };
    for (i, (r, target)) in signatures.iter().zip(expected).enumerate() {
        let functions = compile_routines(&[raw(r)], Options::default(), &mut sql)
            .unwrap_or_else(|error| panic!("routine {i} {}: {error:?}", r["name"]));
        assert_eq!(functions[0].json(), target, "routine {i}: {}", r["name"]);
    }
}
