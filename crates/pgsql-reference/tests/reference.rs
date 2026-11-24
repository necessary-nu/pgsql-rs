use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

// [spec:pgq:req:grammar.parse-speed+1/test]
fn compare_native_corpus(tools: &Path, run: &Path, native: &Path) -> Result<()> {
    let corpus = run.join("regression");
    let regress = tools.join("regress");
    let extraction = Command::new("python3")
        .arg(regress.join("extract.py"))
        .args([run, &corpus])
        .status()?;
    assert!(extraction.success(), "inspect {}", corpus.display());
    let capture = Command::new("python3")
        .arg(regress.join("probes.py"))
        .arg("capture")
        .args([run, &corpus])
        .status()?;
    assert!(capture.success(), "inspect {}", corpus.display());
    let comparison = Command::new(env!("CARGO"))
        .args([
            "test",
            "--locked",
            "-p",
            "pgsql-grammar",
            "--test",
            "corpus",
            "--",
            "--ignored",
        ])
        .env("PG19_NATIVE_PROBE", native)
        .env("PG19_REGRESSION_DIR", &corpus)
        .status()?;
    assert!(comparison.success(), "inspect {}", corpus.display());
    if env::var_os("PG19_BENCHMARK").is_some() {
        let status = Command::new("sh")
            .arg(tools.join("bench/check.sh"))
            .arg(run)
            .arg(&corpus)
            .arg(run.join("benchmark"))
            .status()?;
        assert!(
            status.success(),
            "inspect {}",
            run.join("benchmark").display()
        );
    }
    Ok(())
}

fn check_probe_options(probe: &Path) -> Result<()> {
    let sql = r"SELECT E'it\'s'";
    let default = Command::new(probe).arg(sql).output()?;
    assert!(default.status.success());
    for policy in ["safe", "safe_encoding", "on"] {
        let explicit = Command::new(probe)
            .args([sql, "--backslash-quote", policy, "--mode", "default"])
            .output()?;
        assert!(explicit.status.success());
        assert_eq!(default.stdout, explicit.stdout);
    }
    for flags in [
        &["--mode"][..],
        &["--mode", "unknown"],
        &["--backslash-quote", "unknown"],
        &["--unknown", "off"],
    ] {
        let invalid = Command::new(probe).arg(sql).args(flags).output()?;
        assert_eq!(invalid.status.code(), Some(2));
        assert!(invalid.stdout.is_empty());
    }
    Ok(())
}

// [spec:pgq:req:import.release/test]
// [spec:pgq:req:reference.overlay/test]
// [spec:pgq:req:reference.build/test]
// [spec:pgq:req:corpus.compare/test]
// [spec:pgq:req:codec.schema-import/test]
#[test]
#[ignore = "builds two PostgreSQL backends; set PG19_ARCHIVE for an offline run"]
fn compare_both_references_from_a_clean_import() -> Result<()> {
    let temp = Command::new("mktemp")
        .arg("-d")
        .arg(env::temp_dir().join("pgsql-reference-test.XXXXXXXX"))
        .output()?;
    assert!(temp.status.success());
    let parent = PathBuf::from(String::from_utf8(temp.stdout)?.trim_end());
    let mut command = Command::new(env!("CARGO_BIN_EXE_pgsql-reference"));
    command.arg("import").arg(&parent);
    if let Some(archive) = env::var_os("PG19_ARCHIVE") {
        command.arg(archive);
    }
    let status = command.status()?;
    assert!(status.success(), "inspect {}", parent.display());
    let run = fs::read_dir(&parent)?.next().expect("one import")?.path();
    for flavor in ["stock", "patched"] {
        check_probe_options(&run.join(flavor).join("probe"))?;
    }
    let tools = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/pg19");
    assert!(Command::new("python3")
        .arg(tools.join("regress/test_corpus.py"))
        .arg(&run)
        .status()?
        .success());
    let summary = run.join("summary");
    assert!(Command::new("python3")
        .arg(tools.join("tests/build-summary-probe.py"))
        .arg(&run)
        .arg(&summary)
        .status()?
        .success());
    assert!(Command::new("python3")
        .arg(tools.join("tests/check-summary.py"))
        .arg(summary.join("summary-probe"))
        .arg("--import")
        .arg(&run)
        .status()?
        .success());
    for generator in [
        "import-lexer.py",
        "generate-raw-walk.py",
        "generate-tree-output.py",
        "../../crates/pgsql-nodes/generate.py",
    ] {
        assert!(
            Command::new("python3")
                .arg(tools.join(generator))
                .arg("--check")
                .arg(&run)
                .status()?
                .success(),
            "{generator}"
        );
    }
    for (krate, test) in [
        ("pgsql-lexer", "scan"),
        ("pgsql-engine", "diagnostics"),
        ("pgsql-grammar", "expressions"),
        ("pgsql-grammar", "utility"),
        ("pgsql-reference", "native_tree"),
        ("pgsql-operations", "normalize"),
        ("pgsql-operations", "legacy_deparse"),
    ] {
        assert!(
            Command::new(env!("CARGO"))
                .args([
                    "test",
                    "--locked",
                    "-p",
                    krate,
                    "--test",
                    test,
                    "--",
                    "--ignored"
                ])
                .env("PG19_REFERENCE_DIR", &run)
                .status()?
                .success(),
            "{krate} {test}"
        );
    }
    for (script, args) in [
        ("generate-tables.py", &[][..]),
        ("generate-tables.py", &["--plpgsql"]),
        ("generate-simple-actions.py", &[]),
    ] {
        let generation = Command::new("python3")
            .arg(tools.join(script))
            .arg(run.join("patched-source"))
            .args(args)
            .status()?;
        assert!(generation.success(), "inspect {}", run.display());
    }
    assert!(Command::new("python3")
        .arg(tools.join("generate-record-actions.py"))
        .arg(run.join("patched"))
        .arg("--check")
        .status()?
        .success());
    let schema = Command::new(env!("CARGO_BIN_EXE_pgsql-reference"))
        .arg("check-schema")
        .arg(&run)
        .status()?;
    assert!(schema.success(), "inspect {}", run.display());
    assert!(Command::new("ruby")
        .arg(tools.join("plpgsql/import-types.rb"))
        .arg(&run)
        .status()?
        .success());
    for script in ["plpgsql/build.py", "plpgsql/check.py"] {
        let path = tools.join(script);
        let status = Command::new("python3").arg(path).arg(&run).status()?;
        assert!(status.success(), "inspect {} after {script}", run.display());
    }
    assert!(Command::new(env!("CARGO"))
        .args(["test", "--locked", "-p", "pgsql-plpgsql", "--", "--ignored"])
        .env("PG19_REFERENCE_DIR", &run)
        .status()?
        .success());
    if let Some(native) = env::var_os("PG19_NATIVE_PROBE") {
        compare_native_corpus(&tools, &run, Path::new(&native))?;
    }
    fs::copy(run.join("stock/probe"), run.join("patched/probe"))?;
    let mismatch = Command::new(env!("CARGO_BIN_EXE_pgsql-reference"))
        .arg("compare")
        .arg(&run)
        .output()?;
    assert!(!mismatch.status.success());
    assert!(String::from_utf8(mismatch.stderr)?.contains("differs from the patched probe"));
    fs::remove_dir_all(parent)?;
    Ok(())
}
