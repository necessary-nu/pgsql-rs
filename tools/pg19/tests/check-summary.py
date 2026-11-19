"""Capture or check the PG19 summary wire result for every original summary test input."""
import argparse
import ast
import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SNAPSHOT = ROOT / "crates/pg19-operations/tests/summary.jsonl"
STRING = r'"(?:\\.|[^"\\])*"'
LITERALS = rf'{STRING}(?:\s*{STRING})*'


def c_string(expr):
    return "".join(ast.literal_eval(m[0]) for m in re.finditer(STRING, expr))


def original_inputs():
    # Backslash-newline splicing precedes C tokenization. These tests use only
    # string literals or local names assigned concatenated string literals.
    code = (ROOT / "test/summary_tests.c").read_text().replace("\\\n", "")
    functions = list(re.finditer(r"^void (\w+)\(TestState\* test_state\)", code, re.M))
    assignments = rf'\b(?P<var>sql|query)\s*=\s*(?P<value>{LITERALS})\s*;'
    calls = rf'\bsummary(?:_internal)?\(\s*(?P<arg>{LITERALS}|sql|query)\s*,\s*0\s*,\s*-1\s*\)'
    pattern = re.compile(f"{assignments}|{calls}")
    for i, function in enumerate(functions):
        body = code[function.end():functions[i+1].start() if i+1 < len(functions) else len(code)]
        body = re.sub(r"^\s*//.*$", "", body, flags=re.M)
        values = {}
        count = 0
        for m in pattern.finditer(body):
            if m["var"]:
                values[m["var"]] = c_string(m["value"])
            else:
                arg = m["arg"]
                sql = c_string(arg) if arg.startswith('"') else values[arg]
                yield f"{function[1]}/{count+1}", 0, sql
                count += 1
        if count != len(re.findall(r"\bsummary(?:_internal)?\(", body)):
            raise ValueError(f"unknown summary fixture syntax in {function[1]}")


def inputs():
    yield from original_inputs()
    yield from [
        ("qualified-function-and-filter", 0, "SELECT s.f(a) FROM s.t WHERE s.t.a = 1"),
        ("nested-filter-only", 0, "SELECT * FROM t WHERE a = (SELECT b FROM u WHERE c = 1)"),
        ("quoted-names-and-aliases", 0, 'SELECT "F"("É".x) FROM "both"."T" AS "É" WHERE "É"."?" > 1'),
        ("duplicate-alias", 0, "SELECT a.x FROM t a; SELECT a.y FROM u a"),
        ("multi-statement-types", 0, "EXPLAIN SELECT 1; PREPARE q AS INSERT INTO t VALUES (1); SELECT 2"),
        ("subquery-and-sibling-filter", 0, "SELECT * FROM t WHERE a IN (SELECT b FROM u WHERE c = 1) AND d = 2"),
        ("pg19-repack-and-wait", 0, "REPACK t USING INDEX ix; CLUSTER t; WAIT FOR LSN '0/1'"),
        ("empty", 0, ""),
        ("type-start", 1, "numeric(7,2)[]"),
        ("expression-start", 2, "f(a) FROM t WHERE a = 1"),
        ("assign-one", 3, "a := f(x) FROM t WHERE t.b > 2"),
        ("assign-two", 4, "a.b := f(x)"),
        ("assign-three", 5, "a.b.c := 1"),
    ]


def snapshot(probe):
    rows = []
    for name, mode, sql in inputs():
        result = subprocess.run([probe, str(mode)], input=sql.encode(), capture_output=True)
        row = {"id": name, "mode": mode, "sql": sql}
        if result.returncode == 0:
            row["protobuf"] = result.stdout.hex()
        elif result.returncode == 1:
            row["error"] = json.loads(result.stderr)
        else:
            raise ValueError(f"{name}: reference exited {result.returncode}: {result.stderr!r}")
        rows.append(row)
    return rows


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("probe", type=Path)
    parser.add_argument("--write", action="store_true", help="replace the pinned snapshots")
    args = parser.parse_args()
    rows = snapshot(args.probe.resolve())
    if args.write:
        SNAPSHOT.write_text("".join(json.dumps(row, ensure_ascii=False, separators=(",", ":")) + "\n" for row in rows))
    else:
        expected = [json.loads(line) for line in SNAPSHOT.read_text().splitlines()]
        if rows != expected:
            raise ValueError("summary result differs; inspect the regenerated snapshot")
    print(f"{len(rows)} PG19 summary inputs, {sum('error' in row for row in rows)} diagnostics")
