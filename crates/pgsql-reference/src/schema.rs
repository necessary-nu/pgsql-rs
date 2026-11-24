use super::{
    checksum, files_with_extension, fresh_directory, run, verify_archive, Result, RELEASE, TOOLS,
};
use std::fmt::Write;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const ARTIFACTS: [&str; 4] = [
    "pg_query.proto",
    "pg_query.pb",
    "changes.txt",
    "inputs.sha256",
];

fn descriptor(proto_dir: &Path, out: &Path) -> Result<()> {
    run(Command::new("protoc")
        .current_dir(proto_dir)
        .arg("-I.")
        .arg(format!("--descriptor_set_out={}", out.display()))
        .arg("pg_query.proto"))
}

fn manifest(root: &Path, import: &Path, work: &Path, output: &Path) -> Result<()> {
    let archive = format!("{RELEASE}.tar.bz2");
    let mut inputs = vec![(archive.clone(), import.join(archive))];
    for patch in files_with_extension(&root.join("tools/pg19/patches"), "patch")? {
        inputs.push((
            patch.strip_prefix(root)?.to_string_lossy().into_owned(),
            patch,
        ));
    }
    for file in [
        "libpg_query/scripts/extract_headers.rb",
        "libpg_query/scripts/generate_protobuf_and_funcs.rb",
        "libpg_query/protobuf/pg_query.proto",
    ] {
        inputs.push((file.to_owned(), import.join(file)));
    }
    for file in ["tools/pg19/schema-diff.py", "tools/pg19/libpg_query.patch"] {
        inputs.push((file.to_owned(), root.join(file)));
    }
    // These are the generator's parsed inputs, including the actual token enum
    // from Bison and the node tags from PostgreSQL's generated headers.
    for data in files_with_extension(&work.join("srcdata"), "json")? {
        inputs.push((
            data.strip_prefix(work)?.to_string_lossy().into_owned(),
            data,
        ));
    }
    for name in ["pg_query.proto", "pg_query.pb", "changes.txt"] {
        inputs.push((name.to_owned(), output.join(name)));
    }
    let mut text = String::new();
    for (name, path) in inputs {
        writeln!(text, "{}  {}", checksum(&path)?, name)?;
    }
    fs::write(output.join("inputs.sha256"), text)?;
    Ok(())
}

// [spec:pgq:req:codec.schema-import]
pub(super) fn generate(import: &Path) -> Result<PathBuf> {
    let tools = Path::new(TOOLS);
    let root = tools.join("../..").canonicalize()?;
    let import = import.canonicalize()?;
    verify_archive(&import.join(format!("{RELEASE}.tar.bz2")))?;
    let source = import.join("patched-source");
    let build = import.join("patched");
    let config = fs::read_to_string(build.join("src/include/pg_config.h"))?;
    if !config.lines().any(|l| l == "#define PG_VERSION_NUM 190000") {
        return Err("expected a configured PostgreSQL 19.0 reference build".into());
    }
    let work = fresh_directory(&import)?;
    let output = work.join("protobuf");
    for dir in ["protobuf", "srcdata", "src/include"] {
        fs::create_dir_all(work.join(dir))?;
    }
    run(Command::new("ruby")
        .current_dir(&work)
        .arg(import.join("libpg_query/scripts/extract_headers.rb"))
        .arg(source)
        .arg(build))?;
    let generation = Command::new("ruby")
        .current_dir(&work)
        .arg(import.join("libpg_query/scripts/generate_protobuf_and_funcs.rb"))
        .stderr(Stdio::inherit())
        .output()?;
    let diagnostics = String::from_utf8(generation.stdout)?;
    if !generation.status.success() || diagnostics.lines().any(|line| line.starts_with("ERR:")) {
        return Err(format!(
            "schema generator failed ({}): {diagnostics}",
            generation.status
        )
        .into());
    }
    descriptor(&output, &output.join("pg_query.pb"))?;
    let baseline = work.join("pg18.pb");
    descriptor(&import.join("libpg_query/protobuf"), &baseline)?;
    run(Command::new("python3")
        .arg(tools.join("schema-diff.py"))
        .arg(baseline)
        .arg(output.join("pg_query.pb"))
        .stdout(File::create(output.join("changes.txt"))?))?;
    manifest(&root, &import, &work, &output)?;
    Ok(output)
}

pub(super) fn check(output: &Path) -> Result<()> {
    let pinned = Path::new(TOOLS).join("../../protobuf/pg19");
    for name in ARTIFACTS {
        if fs::read(output.join(name))? != fs::read(pinned.join(name))? {
            return Err(
                format!("PG19 schema differs in {name}; review {}", output.display()).into(),
            );
        }
    }
    println!("Reproduced all four PG19 schema artifacts.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // [spec:pgq:req:codec.schema-import/test]
    #[test]
    fn release_pins_include_schema_descriptor_and_wire_review() -> Result<()> {
        let pinned = Path::new(TOOLS).join("../../protobuf/pg19");
        let manifest = fs::read_to_string(pinned.join("inputs.sha256"))?;
        for name in ["pg_query.proto", "pg_query.pb", "changes.txt"] {
            let sum = checksum(&pinned.join(name))?;
            assert!(manifest
                .lines()
                .any(|line| line == format!("{sum}  {name}")));
        }
        let review = fs::read_to_string(pinned.join("changes.txt"))?;
        assert!(review.contains("message pg_query.ClusterStmt: removed"));
        assert!(review.contains("message pg_query.RepackStmt: added"));
        assert!(review.contains("enum pg_query.OnConflictAction.ONCONFLICT_SELECT: - -> 4"));
        Ok(())
    }

    #[test]
    fn checker_identifies_each_changed_artifact() -> Result<()> {
        let pinned = Path::new(TOOLS).join("../../protobuf/pg19");
        let candidate = fresh_directory(&std::env::temp_dir())?;
        for name in ARTIFACTS {
            fs::copy(pinned.join(name), candidate.join(name))?;
        }
        check(&candidate)?;
        for name in ARTIFACTS {
            fs::write(candidate.join(name), b"modified artifact")?;
            let error = check(&candidate).expect_err("changed artifact must fail verification");
            assert!(error.to_string().contains(name), "{error}");
            fs::copy(pinned.join(name), candidate.join(name))?;
        }
        fs::remove_dir_all(candidate)?;
        Ok(())
    }
}
