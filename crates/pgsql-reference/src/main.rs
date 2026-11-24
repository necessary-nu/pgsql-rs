mod schema;

use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const TOOLS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tools/pg19");
const RELEASE: &str = "postgresql-19.0";
const URL: &str = "https://ftp.postgresql.org/pub/source/v19.0/postgresql-19.0.tar.bz2";
const SHA256: &str = "a89fd70efbad6a08c7dbed8d4872e3af30170d9e5ca9dc7906523be95b4e692d";
const FLAVORS: [&str; 2] = ["stock", "patched"];

fn run(cmd: &mut Command) -> Result<()> {
    let status = cmd.status()?;
    if !status.success() {
        return Err(format!("{cmd:?} exited with {status}").into());
    }
    Ok(())
}

fn checksum(path: &Path) -> Result<String> {
    let output = Command::new("sha256sum")
        .stdin(File::open(path)?)
        .output()?;
    if !output.status.success() {
        return Err(format!("sha256sum failed for {}", path.display()).into());
    }
    Ok(std::str::from_utf8(&output.stdout)?
        .split_whitespace()
        .next()
        .ok_or("missing SHA-256 digest")?
        .to_owned())
}

// [spec:pgq:req:import.checksum]
fn verify_archive(archive: &Path) -> Result<()> {
    if checksum(archive)? != SHA256 {
        return Err(format!(
            "{} failed PostgreSQL 19.0 SHA-256 verification",
            archive.display()
        )
        .into());
    }
    Ok(())
}

// [spec:pgq:req:import.release]
fn get_archive(dir: &Path, local: Option<&OsString>) -> Result<PathBuf> {
    let archive = dir.join(format!("{RELEASE}.tar.bz2"));
    if let Some(local) = local {
        fs::copy(local, &archive)?;
    } else {
        run(Command::new("curl")
            .args([
                "--fail",
                "--location",
                "--show-error",
                "--silent",
                "--output",
            ])
            .arg(&archive)
            .arg(URL))?;
    }
    verify_archive(&archive)?;
    Ok(archive)
}

fn files_with_extension(dir: &Path, extension: &str) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == extension) {
            paths.push(path);
        }
    }
    paths.sort();
    if paths.is_empty() {
        return Err(format!("no .{extension} files in {}", dir.display()).into());
    }
    Ok(paths)
}

// [spec:pgq:req:reference.overlay]
fn apply_overlay(source: &Path) -> Result<()> {
    for patch in files_with_extension(&Path::new(TOOLS).join("patches"), "patch")? {
        run(Command::new("patch")
            .current_dir(source)
            .args(["--batch", "--forward", "--fuzz=0", "-p1", "-i"])
            .arg(patch))?;
    }
    Ok(())
}

// [spec:pgq:req:import.isolation]
fn fresh_directory(parent: &Path) -> Result<PathBuf> {
    fs::create_dir_all(parent)?;
    let parent = parent.canonicalize()?;
    // Each process reserves a fresh directory, including after a failed or interrupted import.
    for number in 1.. {
        let dir = parent.join(format!("pg19-{}-{number}", std::process::id()));
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    unreachable!()
}

// [spec:pgq:req:reference.build]
fn build_probe(dir: &Path, archive: &Path, flavor: &str) -> Result<()> {
    let source = dir.join(format!("{flavor}-source"));
    fs::create_dir(&source)?;
    run(Command::new("tar")
        .arg("-xjf")
        .arg(archive)
        .arg("-C")
        .arg(&source)
        .args(["--strip-components=1", RELEASE]))?;
    if flavor == "patched" {
        apply_overlay(&source)?;
    }
    run(Command::new("sh")
        .arg(Path::new(TOOLS).join("build-reference.sh"))
        .arg(flavor)
        .arg(source)
        .arg(dir.join(flavor)))
}

struct CorpusCase {
    input: PathBuf,
    options: Vec<String>,
}

fn corpus_inputs() -> Result<Vec<CorpusCase>> {
    let dir = Path::new(TOOLS).join("fixtures");
    let manifest = fs::read_to_string(dir.join("cases.txt"))?;
    let mut cases = Vec::new();
    for line in manifest.lines() {
        let mut fields = line.split_ascii_whitespace();
        let name = fields.next().ok_or("empty fixture name in cases.txt")?;
        cases.push(CorpusCase {
            input: dir.join(format!("{name}.sql")),
            options: fields.map(str::to_owned).collect(),
        });
    }
    let inputs = files_with_extension(&dir, "sql")?;
    if !inputs.iter().eq(cases.iter().map(|case| &case.input)) {
        return Err("SQL fixtures differ from the sorted inventory in fixtures/cases.txt".into());
    }
    Ok(cases)
}

// [spec:pgq:req:corpus.compare]
fn compare(dir: &Path) -> Result<()> {
    let inputs = corpus_inputs()?;
    for flavor in FLAVORS {
        let probe = dir.join(flavor).join("probe");
        for case in &inputs {
            let expected = case.input.with_extension(format!("{flavor}.json"));
            let sql = fs::read_to_string(&case.input)?;
            let output = Command::new(&probe)
                .arg(sql)
                .args(&case.options)
                .env("LC_ALL", "C")
                .stderr(Stdio::inherit())
                .output()?;
            if !output.status.success() {
                return Err(format!(
                    "{flavor} probe exited with {} on {}",
                    output.status,
                    case.input.display()
                )
                .into());
            }
            if output.stdout != fs::read(&expected)? {
                return Err(format!(
                    "{} differs from the {flavor} probe\nactual: {}",
                    expected.display(),
                    String::from_utf8_lossy(&output.stdout)
                )
                .into());
            }
        }
    }
    println!(
        "Matched {} SQL fixtures on stock and patched PostgreSQL 19.0.",
        inputs.len()
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    match args.as_slice() {
        [command, dir] if command == "schema" || command == "check-schema" => {
            let output = schema::generate(Path::new(dir))?;
            println!("PG19 schema artifacts: {}", output.display());
            if command == "check-schema" {
                schema::check(&output)?;
                let work = output.parent().expect("schema output directory");
                run(Command::new("python3")
                    .arg(Path::new(TOOLS).join("../../crates/pgsql-codec/generate-json.py"))
                    .arg(work)
                    .arg("--check"))?;
                for check in ["tests/check-json.py", "tests/check-fingerprint.py"] {
                    run(Command::new("python3")
                        .arg(Path::new(TOOLS).join(check))
                        .arg(dir)
                        .arg(work))?;
                }
            }
            Ok(())
        }
        [command, dir] if command == "compare" => compare(Path::new(dir)),
        [command, parent, archive @ ..] if command == "import" && archive.len() <= 1 => {
            let dir = fresh_directory(Path::new(parent))?;
            eprintln!("PostgreSQL 19.0 reference directory: {}", dir.display());
            let tarball = get_archive(&dir, archive.first())?;
            for flavor in FLAVORS {
                build_probe(&dir, &tarball, flavor)?;
            }
            compare(&dir)
        }
        _ => Err(
            "usage: pgsql-reference import <work-parent> [local-tar.bz2] | {compare|schema|check-schema} <run-directory>"
                .into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_registered_inputs_have_both_snapshots() -> Result<()> {
        for case in corpus_inputs()? {
            for flavor in FLAVORS {
                assert!(case
                    .input
                    .with_extension(format!("{flavor}.json"))
                    .is_file());
            }
        }
        Ok(())
    }

    // [spec:pgq:req:import.checksum/test]
    #[test]
    fn rejects_wrong_archive_before_extracting() -> Result<()> {
        let dir = fresh_directory(&env::temp_dir())?;
        let archive = dir.join("other-release");
        fs::write(&archive, "an unrelated archive")?;
        assert!(verify_archive(&archive).is_err());
        fs::remove_dir_all(dir)?;
        Ok(())
    }

    // [spec:pgq:req:import.isolation/test]
    #[test]
    fn repeated_imports_have_separate_source_trees() -> Result<()> {
        let parent = fresh_directory(&env::temp_dir())?;
        let first = fresh_directory(&parent)?;
        let second = fresh_directory(&parent)?;
        assert_ne!(first, second);
        assert!(first.is_dir() && second.is_dir());
        fs::remove_dir_all(parent)?;
        Ok(())
    }
}
