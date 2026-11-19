"""Reparse native compact SQL with the PostgreSQL 19 C JSON probe."""
import argparse
from concurrent.futures import ThreadPoolExecutor
from collections import Counter
import json
from pathlib import Path
import subprocess
import tempfile

LOCATION_FIELDS = {"location", "stmt_len", "list_start", "list_end", "rexpr_list_start",
                   "rexpr_list_end"}


def structural(value):
    if isinstance(value, list):
        return [structural(item) for item in value]
    if isinstance(value, dict):
        return {key: structural(item) for key, item in value.items()
                if key not in LOCATION_FIELDS and not key.endswith("_location")}
    return value


def tree(executable, sql):
    result = subprocess.run([executable, sql], capture_output=True, text=True, timeout=30)
    if result.returncode:
        raise ValueError(result.stderr.strip() or f"probe exited {result.returncode}")
    return structural(json.loads(result.stdout))


def verify(native, probe, cases, jobs, mismatches, require_complete=False):
    counts = Counter()
    errors = Counter()
    rendered = {}
    unrendered = []
    with cases.open("rb") as source, tempfile.TemporaryFile(mode="w+t") as rows:
        subprocess.run([native], stdin=source, stdout=rows, check=True)
        rows.seek(0)
        for line in rows:
            row = json.loads(line)
            counts["default_cases"] += 1
            if row.get("parse_error"):
                counts["native_parse_errors"] += 1
            elif "deparse_error" in row:
                errors[row["deparse_error"]] += 1
                if require_complete:
                    unrendered.append(row)
            else:
                counts["rendered"] += 1
                rendered.setdefault((row["sql"], row["rendered"]), row["id"])

    def compare(item):
        (original, sql), case_id = item
        try:
            before, after = tree(probe, original), tree(probe, sql)
            if before == after:
                return None
            reason = "PG19 trees differ"
        except (ValueError, OSError, subprocess.TimeoutExpired) as exc:
            reason = str(exc)
        return {"id": case_id, "sql": original, "rendered": sql, "error": reason}

    counts["distinct_pairs"] = len(rendered)
    with mismatches.open("w") as output, ThreadPoolExecutor(jobs) as pool:
        for failure in unrendered:
            output.write(json.dumps(failure) + "\n")
        for failure in pool.map(compare, rendered.items()):
            if failure is not None:
                counts["mismatches"] += 1
                output.write(json.dumps(failure) + "\n")
    print(json.dumps({"counts": counts, "unsupported": errors}, sort_keys=True))
    return bool(rendered) and counts["mismatches"] == 0 and (not require_complete or not errors)


if __name__ == "__main__":
    args = argparse.ArgumentParser(description=__doc__)
    args.add_argument("cases", type=Path)
    args.add_argument("native", type=Path)
    args.add_argument("json_probe", type=Path)
    args.add_argument("mismatches", type=Path)
    args.add_argument("--jobs", type=int, default=4)
    args.add_argument("--require-complete", action="store_true",
                      help="also fail if a parsed statement cannot be rendered")
    opts = args.parse_args()
    raise SystemExit(not verify(opts.native, opts.json_probe, opts.cases, opts.jobs,
                                opts.mismatches, opts.require_complete))
