use std::path::Path;
use std::process::Command;

// [spec:pgq:req:grammar.corpus-parity/test]
#[test]
#[ignore = "requires the saved PG19 source and outcome corpus and a release pgsql-native example"]
fn complete_regression_corpus_matches_the_pinned_reference() {
    let corpus = std::env::var_os("PG19_REGRESSION_DIR").expect("PG19_REGRESSION_DIR");
    let native = std::env::var_os("PG19_NATIVE_PROBE").expect("PG19_NATIVE_PROBE");
    let tools = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/pg19/regress");
    assert!(Command::new("python3")
        .arg(tools.join("report.py"))
        .arg(&corpus)
        .arg("--check")
        .status()
        .unwrap()
        .success());
    assert!(Command::new("python3")
        .arg(tools.join("probes.py"))
        .arg("coverage")
        .arg(native)
        .arg(corpus)
        .status()
        .unwrap()
        .success());
}
