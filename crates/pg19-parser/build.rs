// Bind reviewed native reductions to the exact patched gram.y actions.
// Bison rule numbers can change when an unrelated production is inserted.
use std::fmt::Write;
use std::{collections::BTreeMap, env, fs, path::PathBuf};

#[path = "build/omissions.rs"]
mod omissions;
#[path = "build/shifts.rs"]
mod shifts;

type Arcs = [BTreeMap<u16, (u16, usize)>];
type Targets = BTreeMap<(u16, Vec<usize>), Vec<u16>>;

struct Dispatch {
    gotos: String,
    lhs: Vec<usize>,
    omissions: BTreeMap<usize, String>,
}

fn ranges(numbers: &[u16]) -> String {
    numbers
        .chunk_by(|a, b| *a + 1 == *b)
        .map(|run| match run {
            [one] => one.to_string(),
            _ => format!("{}..={}", run[0], run[run.len() - 1]),
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn entries(source: &str) -> BTreeMap<&str, (&str, &str)> {
    let mut entries = BTreeMap::new();
    for line in source
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let mut fields = line.splitn(3, '\t');
        let rule = fields.next().unwrap();
        let hash = fields.next().expect("missing action hash");
        let action = fields.next().expect("missing native action");
        assert!(
            entries.insert(rule, (hash, action)).is_none(),
            "duplicate rule {rule}"
        );
    }
    entries
}

fn unit_actions(chain: &[usize]) -> String {
    chain
        .iter()
        .map(|rule| format!("unit_{rule}(_stack, _span)?; "))
        .collect()
}

/// Several hundred predecessor states can lead to different destinations
/// while sharing the same few semantic actions after the goto.
fn write_goto_table(
    code: &mut String,
    targets: &Targets,
    default: &(u16, Vec<usize>),
    sql_end: Option<&omissions::SqlEnd>,
) {
    let mut chains: Vec<_> = targets.keys().map(|(_, chain)| chain).collect();
    chains.sort_unstable();
    chains.dedup();
    let entry = |(state, chain): &(u16, Vec<usize>)| {
        let id = chains.binary_search(&chain).unwrap();
        format!("(State::from_number({state}).unwrap(), {id})")
    };
    writeln!(code, "static ROW: [(State, u16); pg19_tables::STATE_COUNT] = {{ let mut row = [{}; pg19_tables::STATE_COUNT];", entry(default)).unwrap();
    for (target, sources) in targets {
        if target != default {
            let entry = entry(target);
            for source in sources {
                writeln!(code, "row[{source}] = {entry};").unwrap();
            }
        }
    }
    code.push_str("row }; let (state, _chain) = ROW[usize::from(_previous.number())];\n");
    match chains.as_slice() {
        [chain] => code.push_str(&unit_actions(chain)),
        [empty, chain] if empty.is_empty() => {
            writeln!(code, "if _chain == 1 {{ {} }}", unit_actions(chain)).unwrap();
        }
        _ => {
            code.push_str("match _chain {\n");
            for (id, chain) in chains.iter().enumerate().filter(|(_, c)| !c.is_empty()) {
                writeln!(code, "{id} => {{ {} }},", unit_actions(chain)).unwrap();
            }
            code.push_str("_ => {} }\n");
        }
    }
    if sql_end.is_some_and(|end| targets.keys().any(|(s, _)| *s == end.list.number())) {
        code.push_str("if state == SQL_END.list { return SQL_END.finish(_stack, _span); }\n");
    }
    code.push_str("Ok(state)\n");
}

/// Follow Bison's unconditional one-symbol reductions from each viable goto.
/// Pass-through rules keep the value in place. Other actions run in order
/// against the same stack cell, before the driver requests another token.
fn folded_gotos(handlers: &[Option<&str>], rules: &str) -> Dispatch {
    let symbols = "../pg19-tables/data/symbols.txt";
    let edges = "../pg19-tables/data/gotos.bin";
    println!("cargo:rerun-if-changed={symbols}");
    println!("cargo:rerun-if-changed={edges}");
    let symbols = fs::read_to_string(symbols).unwrap();
    let indexes: BTreeMap<_, _> = symbols.lines().enumerate().map(|(i, s)| (s, i)).collect();
    let mut lhs = vec![0];
    for row in rules.lines() {
        lhs.push(indexes[row.split_once(':').unwrap().0]);
    }
    let mut arcs = vec![BTreeMap::new(); indexes.len()];
    let data = fs::read(edges).unwrap();
    let number = |bytes: &[u8]| u16::from_le_bytes([bytes[0], bytes[1]]);
    let count = usize::from(number(&data));
    let (units, data) = data[2..].split_at(2 * count);
    let units: Vec<_> = units
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| usize::from(number(b)))
        .collect();
    assert!(data.len().is_multiple_of(6));
    for arc in data.as_chunks::<6>().0 {
        let dest = number(&arc[4..]);
        arcs[usize::from(number(arc))].insert(number(&arc[2..]), (dest, units[usize::from(dest)]));
    }
    let mut code = String::new();
    let mut used = std::collections::BTreeSet::new();
    let sql_end = omissions::sql_end(handlers);
    if let Some(end) = &sql_end {
        code.push_str(&format!(
            "const SQL_END: SqlEnd = SqlEnd {{ list: State::from_number({}).unwrap(), before_eof: State::from_number({}).unwrap(), accept: State::from_number({}).unwrap(), append: Production::from_number({}).unwrap(), semicolon: {} }};\n",
            end.list.number(), end.before_eof.number(), end.accept.number(), end.append.number(), end.semicolon
        ));
    }
    for (id, row) in arcs.iter().enumerate().filter(|(_, r)| !r.is_empty()) {
        let mut targets = Targets::new();
        for (&previous, &edge) in row {
            let (mut state, mut unit) = edge;
            let mut sequence = Vec::new();
            let mut visits = 0;
            while unit != 0 && handlers[unit].is_some() {
                visits += 1;
                assert!(visits <= handlers.len(), "cycle in Bison unit reductions");
                if handlers[unit] != Some("Take(1)") {
                    sequence.push(unit);
                    used.insert(unit);
                }
                (state, unit) = arcs[lhs[unit]][&previous];
            }
            targets.entry((state, sequence)).or_default().push(previous);
        }
        let default = targets
            .iter()
            .max_by_key(|(_, from)| from.len())
            .unwrap()
            .0
            .clone();
        code.push_str(&format!("#[inline] fn goto_{id}(_previous: State, _stack: &mut ParseStack<Value>, _span: (usize,usize)) -> Result<State, Box<(Production,Failure)>> {{\n"));
        // Small matches generate less code than a full row of parser states.
        if targets.len() >= 24 {
            write_goto_table(&mut code, &targets, &default, sql_end.as_ref());
            code.push_str("}\n");
            continue;
        }
        targets.remove(&default);
        let finish = |(state, seq): &(u16, Vec<usize>)| {
            let mut body = unit_actions(seq);
            if sql_end
                .as_ref()
                .is_some_and(|end| end.list.number() == *state)
            {
                body.push_str("SQL_END.finish(_stack, _span)");
            } else {
                body.push_str(&format!("Ok(State::from_number({state}).unwrap())"));
            }
            body
        };
        if targets.is_empty() {
            code.push_str(&finish(&default));
        } else {
            code.push_str("match _previous.number() {\n");
            for (target, from) in targets {
                let pattern = ranges(&from);
                code.push_str(&format!("{pattern} => {{ {} }},\n", finish(&target)));
            }
            code.push_str(&format!("_ => {{ {} }} }}\n", finish(&default)));
        }
        code.push_str("}\n");
    }
    for id in used {
        let op = handlers[id].unwrap();
        code.push_str(&format!("#[inline] fn unit_{id}(stack:&mut ParseStack<Value>,span:(usize,usize)) -> Result<(),Box<(Production,Failure)>> {{\n"));
        code.push_str(ACTION_IMPORTS);
        code.push_str(&format!("stack.map_top(span, |rhs| apply({op}, rhs)).map_err(|f| reduction_error({id}, f)) }}\n"));
    }
    Dispatch {
        gotos: code,
        lhs,
        omissions: omissions::guards(handlers, &arcs),
    }
}

const ACTION_IMPORTS: &str = "#[allow(unused_imports)] use Op::*;\n#[allow(unused_imports)] use crate::{raw::Node,expr::Expr as E,query::Query as Q,value::Quantifier as Qu};\n#[allow(unused_imports)] use pg19_nodes::{AExprKind,BoolExprType,BoolTestType,NullTestType,SetOperation};\n";

fn main() {
    let inventory = "../pg19-tables/data/productions.tsv";
    let manifests = "actions";
    println!("cargo:rerun-if-changed={inventory}");
    println!("cargo:rerun-if-changed={manifests}");
    let table = fs::read_to_string(inventory).unwrap();
    println!("cargo:rerun-if-changed=simple.tsv");
    let simple = fs::read_to_string("simple.tsv").unwrap();
    println!("cargo:rerun-if-changed=records.tsv");
    let records = fs::read_to_string("records.tsv").unwrap();
    let generated_sources = format!("{simple}\n{records}");
    let mut generated = entries(&generated_sources);
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
    let mut actions = entries(&source);
    let mut handlers = vec![None; table.lines().count() + 1];

    for (i, row) in table.lines().enumerate() {
        let mut fields = row.split('\t');
        let rule = fields.next().unwrap();
        let hash = fields.nth(1).unwrap();
        let fallback = generated.remove(rule);
        if let Some((pinned, _)) = fallback {
            assert_eq!(
                pinned, hash,
                "generated PostgreSQL action changed for {rule}"
            );
        }
        if hash == "default" {
            handlers[i + 1] = Some("Take(1)");
        }
        if let Some((pinned, action)) = actions.remove(rule).or(fallback) {
            assert_eq!(pinned, hash, "PostgreSQL action changed for {rule}");
            assert_ne!(hash, "default", "engine owns Bison default for {rule}");
            handlers[i + 1] = Some(action);
        }
    }
    assert!(
        actions.is_empty(),
        "PostgreSQL removed mapped productions: {:?}",
        actions.keys()
    );
    assert!(
        generated.is_empty(),
        "PostgreSQL removed generated productions: {:?}",
        generated.keys()
    );
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(out_dir.join("shifts.bin"), shifts::compile(&handlers)).unwrap();
    // The native operations are compile-time constants. Resolve their dispatch
    // here, so a reduction loads one handler directly by Bison production id.
    let mut code = String::from("static HANDLERS: &[Option<Handler>] = {\n");
    code.push_str(ACTION_IMPORTS);
    code.push_str("&[\n");
    let dispatch = folded_gotos(&handlers, &table);
    for (id, handler) in handlers.into_iter().enumerate() {
        let goto = format!("goto_{}", dispatch.lhs[id]);
        let guard = dispatch.omissions.get(&id).map_or("", String::as_str);
        // The accepting state ends parsing before Bison reduces this rule.
        if id == 1 {
            code.push_str("None,\n");
            continue;
        }
        let rule = format!("Production::from_number({id}).unwrap()");
        match handler {
            Some("Take(1)") => code.push_str(&format!(
                "        Some(|stack, current, span| {{ {guard}let previous = stack.reduce_default({rule}, current, |p| p); {goto}(previous, stack, span) }}),\n"
            )),
            Some(op) => code.push_str(&format!(
                "        Some(|stack, current, span| {{ {guard}let previous = stack.reduce({rule}, current, span, |p| p, |rhs| apply({op}, rhs)).map_err(|f| reduction_error({id}, f))?; {goto}(previous, stack, span) }}),\n"
            )),
            None => code.push_str("        None,\n"),
        }
    }
    code.push_str("    ]\n};\n");
    code.push_str(&dispatch.gotos);
    fs::write(out_dir.join("actions.rs"), code).unwrap();
}
