"""Check original normalization vectors and PG19-specific cases against the C operation."""
import argparse
import ast
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


def legacy(mode):
    filename = "normalize_tests.c" if mode == "full" else "normalize_utility_tests.c"
    strings = [ast.literal_eval(line.strip().removesuffix(",")) for line in
               (ROOT / "test" / filename).read_text().splitlines() if line.lstrip().startswith('"')]
    if len(strings) % 2:
        raise ValueError(f"incomplete fixture in {filename}")
    return dict(zip(strings[::2], strings[1::2]))


def check(probe):
    for mode in ["full", "utility"]:
        vectors = legacy(mode)
        for line in (ROOT / "crates/pg19-operations/tests/normalize.jsonl").read_text().splitlines():
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
