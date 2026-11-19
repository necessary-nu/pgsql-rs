"""Compare complete Rust and C PL/pgSQL entry points on upstream PL regression SQL."""
import argparse
import gzip
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


def core_routines(corpus):
    manifest = json.loads((corpus / "inventory.json").read_bytes())
    inputs = corpus / "cases.jsonl"
    expected = json.loads((HERE.parent / "regress/pg19.0-summary.json").read_bytes())
    if (manifest["archive_sha256"] != ARCHIVE_SHA256 or manifest["cases_sha256"] != expected["cases_sha256"]
            or digest(inputs.read_bytes()) != manifest["cases_sha256"]):
        raise ValueError("core corpus archive or SQL inputs differ from the extraction manifest")
    with inputs.open(encoding="utf-8") as rows, gzip.open(corpus / "outcomes.jsonl.gz", "rt", encoding="utf-8") as outcomes:
        for row, outcome in zip(map(json.loads, rows), map(json.loads, outcomes), strict=True):
            if row["id"] != outcome["id"]:
                raise ValueError("core corpus case and outcome IDs differ")
            if row["family"] != "postgres":
                continue
            tree = outcome["patched"]["tree"] or ""
            if "{RAWSTMT :stmt {CREATEFUNCTIONSTMT" in tree or "{RAWSTMT :stmt {DOSTMT" in tree:
                yield row


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


def run_suite(run, native, out, jobs, boundaries, core):
    out.mkdir(parents=True, exist_ok=True)
    if digest((run / (RELEASE + ".tar.bz2")).read_bytes()) != ARCHIVE_SHA256:
        raise ValueError("unexpected PostgreSQL source archive")
    if boundaries is None:
        boundaries = out / "boundaries"
        subprocess.run(["sh", HERE.parent / "regress/build-boundaries.sh", run, boundaries], check=True)
    regions = list(extract(run, boundaries))
    cases = [r for r in regions if r["kind"] == "sql"]
    core_count = 0
    if core:
        extra = list(core_routines(core))
        cases.extend(extra)
        core_count = len(extra)
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
              "reference_results_sha256": witness.hexdigest(), "queries": len(cases), "core_routine_queries": core_count,
              "regions": dict(sorted(Counter(r["kind"] for r in regions).items())),
              "outcomes": dict(sorted(counts.items()))}
    expected = json.loads((HERE / "fixtures/regress-witness.json").read_bytes())
    scope = "pl_and_core" if core else "pl"
    witness_matches = all(result[key] == value for key, value in expected[scope].items())
    result["reference_matches_checked_witness"] = witness_matches
    payload = json.dumps(result, indent=2) + "\n"
    (out / "summary.json").write_text(payload)
    print(payload, end="")
    return witness_matches and all(k.startswith("matched-") for k in counts)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path)
    parser.add_argument("native", type=Path)
    parser.add_argument("out", type=Path)
    parser.add_argument("--boundaries", type=Path, help="existing helper from regress/build-boundaries.sh")
    parser.add_argument("--core-corpus", type=Path, help="also compare routine-bearing SQL from an existing core regress capture")
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--require-parity", action="store_true")
    args = parser.parse_args()
    exact = run_suite(args.run.resolve(), args.native.resolve(), args.out.resolve(), args.jobs,
                      args.boundaries.resolve() if args.boundaries else None, args.core_corpus)
    sys.exit(1 if args.require_parity and not exact else 0)
