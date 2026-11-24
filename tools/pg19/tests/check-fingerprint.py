"""Rebuild the fingerprint field visitor and check versioned digests against PG19."""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]


def check(run, work):
    work = work.resolve()
    subprocess.run(["ruby", ROOT / "tools/pg19/libpg_query/scripts/generate_fingerprint_outfuncs.rb"], cwd=work, check=True)
    subprocess.run(["python3", ROOT / "crates/pgsql-operations/generate-fingerprint.py",
                    work, "--check"], check=True)
    probe = work / "fingerprint-probe"
    subprocess.run(["make", "-s", "-C", run.resolve() / "patched/src/backend",
                    "-f", "Makefile", "-f", ROOT / "tools/pg19/tests/fingerprint-probe.mk",
                    f"work={work}", f"repo={ROOT}", probe, work / "normalize-probe"], check=True)
    subprocess.run(["python3", ROOT / "tools/pg19/tests/check-normalize.py",
                    work / "normalize-probe"], check=True)
    rows = (ROOT / "crates/pgsql-operations/tests/fingerprint-v3.jsonl").read_text().splitlines()
    if not rows:
        raise ValueError("missing fingerprint vectors")
    for row in map(json.loads, rows):
        result = subprocess.run([probe, str(row["options"]), "0"], input=row["sql"],
                                capture_output=True, text=True, check=True)
        if result.stdout.strip() != row["hex"] or row["version"] != 3:
            raise ValueError(f"PG19 fingerprint differs for {row}: {result.stdout} {result.stderr}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path)
    parser.add_argument("schema_work", type=Path)
    args = parser.parse_args()
    check(args.run, args.schema_work)
