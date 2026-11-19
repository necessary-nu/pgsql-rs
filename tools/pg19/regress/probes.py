"""Capture both PostgreSQL outcomes or compare a compatible executable to one."""
import argparse
import gzip
import json
import os
import subprocess
from concurrent.futures import ThreadPoolExecutor
from itertools import islice
from pathlib import Path

from extract import line


def invoke(probe, case):
    cmd = [probe, case["sql"], "--mode", case["mode"], "--backslash-quote", case["backslash_quote"]]
    try:
        result = subprocess.run(cmd, capture_output=True, check=True, timeout=20,
                                env={**os.environ, "LC_ALL": "C"})
        data = json.loads(result.stdout)
        if set(data) != {"tree", "parse_error", "tokens", "scan_error"}:
            raise ValueError("unexpected probe response fields")
        return data
    except (OSError, ValueError, subprocess.SubprocessError) as e:
        raise RuntimeError(f"{case['id']} failed on {probe}: {e}") from e


def parallel(function, rows, jobs):
    with ThreadPoolExecutor(max_workers=jobs) as workers:
        while batch := list(islice(rows, 256)):
            yield from workers.map(function, batch)


def rows(path):
    with path.open(encoding="utf-8") as data:
        yield from map(json.loads, data)


def capture(run, out, jobs):
    def both(case):
        return {"id": case["id"], **{flavor: invoke(run / flavor / "probe", case)
                  for flavor in ("stock", "patched")}}
    target = out / "outcomes.jsonl.gz"
    temp = target.with_suffix(".tmp")
    try:
        with temp.open("wb") as raw, gzip.GzipFile(filename="", fileobj=raw, mode="wb", compresslevel=1, mtime=0) as sink:
            for row in parallel(both, rows(out / "cases.jsonl"), jobs):
                sink.write(line(row).encode())
        temp.replace(target)
    finally:
        temp.unlink(missing_ok=True)


def compare(candidate, out, reference, jobs):
    def check(pair):
        case, expected = pair
        if case["id"] != expected["id"]:
            raise ValueError(f"case/outcome ID mismatch at {case['id']}")
        actual = invoke(candidate, case)
        return case["id"], [k for k, value in expected[reference].items() if actual.get(k) != value]
    count = 0
    errors = 0
    with gzip.open(out / "outcomes.jsonl.gz", "rt", encoding="utf-8") as f:
        pairs = zip(rows(out / "cases.jsonl"), map(json.loads, f), strict=True)
        for case_id, fields in parallel(check, pairs, jobs):
            count += 1
            if fields:
                print(line({"id": case_id, "different": fields}), end="")
                errors += 1
    print(f"{count - errors}/{count} cases match {reference}.")
    return errors == 0


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    cap = sub.add_parser("capture")
    cap.add_argument("run", type=Path)
    cmp = sub.add_parser("compare")
    cmp.add_argument("candidate", type=Path)
    cmp.add_argument("--reference", choices=("stock", "patched"), default="patched")
    for arg in (cap, cmp):
        arg.add_argument("out", type=Path)
        arg.add_argument("--jobs", type=int, default=4)
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error("--jobs must be positive")
    if args.command == "capture":
        capture(args.run.resolve(), args.out, args.jobs)
    elif not compare(args.candidate.resolve(), args.out, args.reference, args.jobs):
        raise SystemExit(1)
