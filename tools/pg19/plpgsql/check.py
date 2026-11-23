"""Compare PL/pgSQL results to the project's sample and pinned PG19 cases."""
import argparse
import hashlib
import json
import subprocess
from pathlib import Path

HERE = Path(__file__).resolve().parent
LIBPG_QUERY = HERE.parent / "libpg_query"
FIXTURES = HERE / "fixtures"


def inputs(run):
    for case in json.loads((FIXTURES / "cases.json").read_text()):
        if "sql" in case:
            sql = case["sql"]
        else:
            data = (run / "patched-source" / case["path"]).read_bytes()[case["start"]:case["end"]]
            if hashlib.sha256(data).hexdigest() != case["sha256"]:
                raise ValueError(f"upstream body differs for {case['id']}")
            sql = data.decode()
        yield case["id"], sql


def evaluate(probe, queries):
    result = subprocess.run([probe, *queries], check=True, capture_output=True, timeout=60)
    return [json.loads(line) for line in result.stdout.splitlines()]


def compare(run, probe):
    sample = (LIBPG_QUERY / "test/plpgsql_samples.sql").read_text()
    sample_tree = json.loads((LIBPG_QUERY / "test/plpgsql_samples.expected.json").read_bytes())
    cases = list(inputs(run))
    # A valid routine follows all deliberate failures in the same process.
    values = evaluate(probe, [sql for name, sql in cases] + [sample])
    if values.pop() != {"functions": sample_tree, "error": None}:
        raise ValueError("test/plpgsql_samples.sql differs after parsing the error fixtures")
    expected = json.loads((FIXTURES / "expected.json").read_bytes())
    if set(expected) != {name for name, _ in cases}:
        raise ValueError("case IDs differ from PL/pgSQL expected results")
    for (name, _), got in zip(cases, values, strict=True):
        if got != expected[name]:
            raise ValueError(f"{name} differs: {json.dumps(got, ensure_ascii=False)}")
    print(f"Matched {len(cases)} PG19 cases and {len(sample_tree)} sample routines.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path)
    parser.add_argument("--candidate", type=Path)
    args = parser.parse_args()
    compare(args.run, (args.candidate or args.run / "plpgsql-probe").resolve())
