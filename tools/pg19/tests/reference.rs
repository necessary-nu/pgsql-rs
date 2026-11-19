use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

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
        .arg(env::temp_dir().join("pg19-reference-test.XXXXXXXX"))
        .output()?;
    assert!(temp.status.success());
    let parent = PathBuf::from(String::from_utf8(temp.stdout)?.trim_end());
    let mut command = Command::new(env!("CARGO_BIN_EXE_pg19-reference"));
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
    let tools = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(Command::new("python3")
        .arg(tools.join("regress/test_corpus.py"))
        .arg(&run)
        .status()?
        .success());
    for generator in [
        "import-lexer.py",
        "generate-tree-output.py",
        "../../crates/pg19-nodes/generate.py",
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
        ("pg19-lexer", "scan"),
        ("pg19-engine", "diagnostics"),
        ("pg19-reference", "native_tree"),
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
    let generator = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("generate-tables.py");
    let generation = Command::new("python3")
        .arg(generator)
        .arg(run.join("patched-source"))
        .status()?;
    assert!(generation.success(), "inspect {}", run.display());
    let schema = Command::new(env!("CARGO_BIN_EXE_pg19-reference"))
        .arg("check-schema")
        .arg(&run)
        .status()?;
    assert!(schema.success(), "inspect {}", run.display());
    for script in ["plpgsql/build.py", "plpgsql/check.py"] {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(script);
        let status = Command::new("python3").arg(path).arg(&run).status()?;
        assert!(status.success(), "inspect {} after {script}", run.display());
    }
    fs::copy(run.join("stock/probe"), run.join("patched/probe"))?;
    let mismatch = Command::new(env!("CARGO_BIN_EXE_pg19-reference"))
        .arg("compare")
        .arg(&run)
        .output()?;
    assert!(!mismatch.status.success());
    assert!(String::from_utf8(mismatch.stderr)?.contains("differs from the patched probe"));
    fs::remove_dir_all(parent)?;
    Ok(())
}
