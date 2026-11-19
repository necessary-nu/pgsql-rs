"""Link libpg_query's JSON writer to the patched PG19 backend and verify its fixtures."""
import argparse
import os
from pathlib import Path
import subprocess
import sys


def check(run, work):
    repo = Path(__file__).resolve().parents[3]
    work = work.resolve()
    probe = work / "json-probe"
    subprocess.run([
        "make", "-s", "-C", run.resolve() / "patched/src/backend",
        "-f", "Makefile", "-f", repo / "tools/pg19/tests/json-probe.mk",
        f"repo={repo}", f"work={work}", probe,
    ], check=True)
    cases = sorted((repo / "crates/pg19-codec/tests").glob("*.sql"))
    if not cases:
        raise SystemExit("no JSON probe fixtures")
    for sql in cases:
        actual = subprocess.check_output([probe, sql.read_text()])
        if actual != sql.with_suffix(".json").read_bytes():
            raise SystemExit(f"PG19 C JSON differs for {sql.name}: {actual!r}")
    subprocess.run([sys.executable, repo / "tools/pg19/tests/check-deparse.py", probe], check=True)
    if native := os.environ.get("PG19_DEPARSE_PROBE"):
        regress = repo / "tools/pg19/regress"
        corpus = work / "deparse-regression"
        subprocess.run([sys.executable, regress / "extract.py", run, corpus], check=True)
        subprocess.run([
            sys.executable, regress / "deparse.py", corpus / "cases.jsonl", native, probe,
            corpus / "deparse-mismatches.jsonl", "--require-complete",
            "--jobs", os.environ.get("PG19_JOBS", "2"),
        ], check=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path)
    parser.add_argument("schema_work", type=Path)
    args = parser.parse_args()
    check(args.run, args.schema_work)
