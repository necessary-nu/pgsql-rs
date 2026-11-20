use std::hint::black_box;
use std::time::Instant;

#[cfg(feature = "bench-mimalloc")]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn run(queries: &[&str], calls: usize, json: bool, dump: bool) -> usize {
    let mut total = 0;
    for query in queries.iter().cycle().take(calls) {
        let tree = pg19_query::parse_statements(query, Default::default()).unwrap();
        total += tree.len();
        if json {
            let output = pg19_query::codec::statements_json(&tree).unwrap();
            drop(tree);
            if dump {
                println!("{output}");
            }
            black_box(output);
        }
    }
    total
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let data = std::fs::read_to_string(&args[2]).unwrap();
    let queries: Vec<_> = data.strip_suffix('\0').unwrap().split('\0').collect();
    let calls = queries.len() * args[3].parse::<usize>().unwrap();
    let mode = args[1].as_str();
    assert!(matches!(mode, "parse" | "json" | "verify"));
    if mode == "verify" {
        run(&queries, queries.len(), true, true);
        return;
    }
    let json = mode == "json";
    black_box(run(&queries, (calls / 10).max(1), json, false));
    let start = Instant::now();
    let statements = run(&queries, calls, json, false);
    let ns = start.elapsed().as_nanos();
    println!("{ns} {calls} {statements}");
}
