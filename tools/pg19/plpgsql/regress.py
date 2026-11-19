"""Compare complete Rust and C PL/pgSQL entry points on upstream PL regression SQL."""
import argparse
import hashlib
import json
import subprocess
import sys
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "regress"))
from extract import ARCHIVE_SHA256, RELEASE, digest, line, source_spans


def extract(run, boundaries):
    source = run / "stock-source"
    for path in sorted((source / "src/pl/plpgsql/src/sql").glob("*.sql")):
        raw = path.read_bytes()
        for kind, start, end in source_spans(boundaries, raw):
            yield {"id": f"{path.relative_to(source)}:{start}-{end}", "kind": kind,
                   "sql": raw[start:end].decode() if kind == "sql" else None}


def invoke(program, sql):
    try:
        run = subprocess.run([program, sql], capture_output=True, timeout=20)
    except subprocess.TimeoutExpired:
        return {"process_error": "timeout"}
    if run.returncode:
        return {"process_error": f"exit {run.returncode}",
                "stderr": run.stderr.decode(errors="replace")[:1000]}
    # #option dump can print an execution tree in addition to the oracle JSON.
    for candidate in reversed(run.stdout.splitlines()):
        if candidate.startswith(b'{"functions"') or candidate.startswith(b'{"error"'):
            return json.loads(candidate)
    raise ValueError(f"no JSON result from {program} for {sql!r}")


def compare(case, oracle, rust):
    sql = case["sql"]
    reference, native = invoke(oracle, sql), invoke(rust, sql)
    if "process_error" in reference:
        kind = "reference-process-error"
    elif "process_error" in native:
        kind = "native-process-error"
    elif "unsupported" in native:
        kind = "unsupported"
    elif native != reference:
        kind = "different-error" if native.get("error") or reference.get("error") else "different-tree"
    elif reference.get("error"):
        kind = "matched-error"
    elif reference["functions"]:
        kind = "matched-routine"
    else:
        kind = "matched-empty"
    return {"id": case["id"], "kind": kind, "sql": sql, "reference": reference, "native": native}


def run_suite(run, native, out, jobs, boundaries):
    out.mkdir(parents=True, exist_ok=True)
    if digest((run / (RELEASE + ".tar.bz2")).read_bytes()) != ARCHIVE_SHA256:
        raise ValueError("unexpected PostgreSQL source archive")
    if boundaries is None:
        boundaries = out / "boundaries"
        subprocess.run(["sh", HERE.parent / "regress/build-boundaries.sh", run, boundaries], check=True)
    regions = list(extract(run, boundaries))
    cases = [r for r in regions if r["kind"] == "sql"]
    if not cases:
        raise ValueError("no PL regression cases")
    counts = Counter()
    witness = hashlib.sha256()
    with ThreadPoolExecutor(jobs) as pool, (out / "differences.jsonl").open("w") as details:
        for record in pool.map(lambda c: compare(c, run / "plpgsql-probe", native), cases):
            counts[record["kind"]] += 1
            witness.update(line({"id": record["id"], "reference": record["reference"]}).encode())
            if not record["kind"].startswith("matched-"):
                details.write(line(record))
    result = {"upstream_release": RELEASE, "archive_sha256": ARCHIVE_SHA256,
              "reference_results_sha256": witness.hexdigest(), "queries": len(cases),
              "regions": dict(sorted(Counter(r["kind"] for r in regions).items())),
              "outcomes": dict(sorted(counts.items()))}
    payload = json.dumps(result, indent=2) + "\n"
    (out / "summary.json").write_text(payload)
    print(payload, end="")
    return all(k.startswith("matched-") for k in counts)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path)
    parser.add_argument("native", type=Path)
    parser.add_argument("out", type=Path)
    parser.add_argument("--boundaries", type=Path, help="existing helper from regress/build-boundaries.sh")
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--require-parity", action="store_true")
    args = parser.parse_args()
    exact = run_suite(args.run.resolve(), args.native.resolve(), args.out.resolve(), args.jobs,
                      args.boundaries.resolve() if args.boundaries else None)
    sys.exit(1 if args.require_parity and not exact else 0)
