"""Check original normalization vectors and PG19-specific cases against the C operation."""
import argparse
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


def check(probe):
    fixtures = ROOT / "crates/pgsql-operations/tests"
    for mode in ["full", "utility"]:
        vectors = {}
        for file in ["normalize-legacy.jsonl", "normalize.jsonl"]:
            for line in (fixtures / file).read_text().splitlines():
                row = json.loads(line)
                vectors[row["sql"]] = row[mode]
        for sql, expected in vectors.items():
            result = subprocess.run([probe, mode], input=sql.encode(), capture_output=True, check=True)
            if result.stdout != expected.encode():
                raise ValueError(f"{mode}: {sql!r}: {expected!r} differs from PG19 {result.stdout!r}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("probe", type=Path)
    check(parser.parse_args().probe)
