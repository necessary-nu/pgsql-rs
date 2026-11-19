// Bind reviewed native reductions to the exact patched gram.y actions.
// Bison rule numbers can change when an unrelated production is inserted.
use std::{collections::BTreeMap, env, fs, path::PathBuf};

fn main() {
    let inventory = "../pg19-tables/data/productions.tsv";
    let manifests = "actions";
    println!("cargo:rerun-if-changed={inventory}");
    println!("cargo:rerun-if-changed={manifests}");
    let table = fs::read_to_string(inventory).unwrap();
    let mut actions = BTreeMap::new();
    let mut paths: Vec<_> = fs::read_dir(manifests)
        .unwrap()
        .map(|file| file.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "tsv"))
        .collect();
    paths.sort();
    let source: String = paths
        .iter()
        .map(|path| {
            let mut text = fs::read_to_string(path).unwrap();
            text.push('\n');
            text
        })
        .collect();
    for line in source
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let mut fields = line.splitn(3, '\t');
        let rule = fields.next().unwrap();
        let hash = fields.next().expect("missing action hash");
        let action = fields.next().expect("missing native action");
        assert!(
            actions.insert(rule, (hash, action)).is_none(),
            "duplicate rule {rule}"
        );
    }
    let mut arms = BTreeMap::<&str, Vec<usize>>::new();
    for (i, row) in table.lines().enumerate() {
        let mut fields = row.split('\t');
        let rule = fields.next().unwrap();
        let hash = fields.nth(1).unwrap();
        if let Some((pinned, action)) = actions.remove(rule) {
            assert_eq!(pinned, hash, "PostgreSQL action changed for {rule}");
            assert_ne!(hash, "default", "engine owns Bison default for {rule}");
            arms.entry(action).or_default().push(i + 1);
        }
    }
    assert!(
        actions.is_empty(),
        "PostgreSQL removed mapped productions: {:?}",
        actions.keys()
    );
    let mut code = String::from("fn action(id: u16) -> Option<Op> {\n    use Op::*;\n    use crate::{expr::Expr as E, query::Query as Q, value::Quantifier as Qu};\n    use pg19_nodes::{AExprKind, BoolExprType, BoolTestType, NullTestType, SetOperation};\n    Some(match id {\n");
    for (op, numbers) in arms {
        let patterns: Vec<_> = numbers
            .chunk_by(|a, b| *a + 1 == *b)
            .map(|run| {
                if run.len() == 1 {
                    run[0].to_string()
                } else {
                    format!("{}..={}", run[0], run[run.len() - 1])
                }
            })
            .collect();
        code.push_str(&format!("        {} => {op},\n", patterns.join(" | ")));
    }
    code.push_str("        _ => return None,\n    })\n}\n");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("actions.rs");
    fs::write(out, code).unwrap();
}
