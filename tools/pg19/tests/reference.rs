use std::env;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

// [spec:pgq:req:import.release/test]
// [spec:pgq:req:reference.overlay/test]
// [spec:pgq:req:reference.build/test]
// [spec:pgq:req:corpus.compare/test]
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
    let generator = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("generate-tables.py");
    let generation = Command::new("python3")
        .arg(generator)
        .arg(run.join("patched-source"))
        .status()?;
    assert!(generation.success(), "inspect {}", run.display());
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
