"""Link libpg_query's JSON writer to the patched PG19 backend and verify its fixtures."""
import argparse
from pathlib import Path
import subprocess


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
    subprocess.run(["python3", repo / "tools/pg19/tests/check-deparse.py", probe], check=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path)
    parser.add_argument("schema_work", type=Path)
    args = parser.parse_args()
    check(args.run, args.schema_work)
