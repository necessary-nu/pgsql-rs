use pg19_nodes::{
    CreateFunctionStmt, DefElem, DoStmt, FunctionParameter, FunctionParameterMode as Mode, Node,
    RawStmt, TypeName,
};
use pg19_plpgsql::{compile_routines, parse_routines, NativeSql, Options, Routine, RoutineError};
use serde_json::{json, Value};

fn rows() -> impl Iterator<Item = Value> {
    include_str!("routines.jsonl")
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
}

fn text(s: &str) -> Node {
    Node::String(s.into())
}
fn opt(s: &str, value: Node) -> Node {
    DefElem {
        defname: Some(s.into()),
        arg: Some(Box::new(value)),
        ..Default::default()
    }
    .into()
}
fn ty(s: &str) -> Box<TypeName> {
    let actual = s.strip_prefix("setof ").unwrap_or(s);
    Box::new(TypeName {
        names: vec![text(actual)],
        setof: actual.len() != s.len(),
        ..Default::default()
    })
}

fn projected_type(v: &Value) -> Box<TypeName> {
    if let Some(s) = v.as_str() {
        return ty(s);
    }
    Box::new(TypeName {
        names: v["names"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| text(n.as_str().unwrap()))
            .collect(),
        type_oid: v["oid"].as_u64().unwrap() as u32,
        setof: v["setof"] == true,
        pct_type: v["pct"] == true,
        array_bounds: vec![Node::Integer(-1); v["bounds"].as_u64().unwrap() as usize],
        ..Default::default()
    })
}

fn raw(r: &Value) -> Node {
    let mut args = Vec::new();
    let is_do = r["kind"] == "do";
    if let Some(body) = r["body"].as_str() {
        args.push(opt(
            "as",
            if is_do {
                text(body)
            } else {
                Node::from(vec![text(body)])
            },
        ));
    }
    if let Some(lang) = r["lang"].as_str() {
        args.push(opt("language", text(lang)));
    }
    let stmt: Node = if is_do {
        DoStmt { args }.into()
    } else {
        let params = r["params"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                let mode = match p["mode"].as_str() {
                    Some("out") => Mode::FUNC_PARAM_OUT,
                    _ if p["mode"].is_number() => Mode(p["mode"].as_i64().unwrap() as i32),
                    Some("inout") => Mode::FUNC_PARAM_INOUT,
                    _ => Mode::FUNC_PARAM_IN,
                };
                FunctionParameter {
                    name: p["name"].as_str().map(str::to_owned),
                    arg_type: Some(projected_type(&p["type"])),
                    defexpr: match p["default"].as_str() {
                        Some("constant") => Some(Box::new(pg19_nodes::AConst::default().into())),
                        Some(_) => Some(Box::new(text("expression"))),
                        None => None,
                    },
                    mode,
                    ..Default::default()
                }
                .into()
            })
            .collect();
        CreateFunctionStmt {
            funcname: vec![text(r["name"].as_str().unwrap())],
            parameters: params,
            options: args,
            return_type: (!r["ret"].is_null()).then(|| projected_type(&r["ret"])),
            sql_body: (r["sql_body"] == true).then(|| Box::new(text("sql_body"))),
            is_procedure: r["kind"] == "procedure",
            ..Default::default()
        }
        .into()
    };
    RawStmt {
        stmt: Some(Box::new(stmt)),
        ..Default::default()
    }
    .into()
}

fn outcome(result: Result<Vec<Routine>, RoutineError>) -> Value {
    match result {
        Ok(routines) => {
            json!({"functions":routines.iter().map(Routine::json).collect::<Vec<_>>(),"error":null})
        }
        Err(e) => {
            let (err, context) = e.diagnostic().unwrap_or_else(|| panic!("{e:?}"));
            let outer = matches!(e, RoutineError::OuterSql(_));
            json!({"functions":null,"error":{"phase":if outer{"sql"}else{"plpgsql"},"sqlstate":err.sqlstate,
                "message":err.message,"position":if outer{err.position}else{0},"internal_position":if outer{0}else{err.position},"context":context}})
        }
    }
}

#[test]
fn native_routine_setup_matches_c() {
    for r in rows() {
        let result = compile_routines(&[raw(&r)], Options::default(), &mut NativeSql::default());
        assert_eq!(outcome(result), r["expected"], "{}", r["sql"]);
    }
}

#[test]
fn sql_entrypoint_accepts_ordinary_sql() {
    assert_eq!(
        parse_routines("SELECT 1;", Options::default()),
        Ok(Vec::new())
    );
    let rows: Vec<_> = rows()
        .filter(|r| matches!(r["id"].as_str(), Some("inline" | "sql_only" | "named")))
        .collect();
    let nodes: Vec<_> = rows.iter().map(raw).collect();
    let result = compile_routines(&nodes, Options::default(), &mut NativeSql::default()).unwrap();
    let expected: Vec<_> = rows
        .iter()
        .map(|v| v["expected"]["functions"][0].clone())
        .collect();
    assert_eq!(
        result.iter().map(Routine::json).collect::<Vec<_>>(),
        expected
    );
}

#[test]
#[ignore = "requires the PostgreSQL 19 PL/pgSQL oracle"]
fn verify_routine_reference_fixtures() {
    let rows: Vec<_> = rows().collect();
    let path = std::path::PathBuf::from(std::env::var_os("PG19_REFERENCE_DIR").unwrap())
        .join("plpgsql-probe");
    let out = std::process::Command::new(path)
        .args(rows.iter().map(|r| r["sql"].as_str().unwrap()))
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(text.lines().count(), rows.len());
    for (r, line) in rows.iter().zip(text.lines()) {
        assert_eq!(serde_json::from_str::<Value>(line).unwrap(), r["expected"]);
    }
}

#[path = "support/samples.rs"]
mod samples;
