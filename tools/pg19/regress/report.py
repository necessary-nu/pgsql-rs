"""Review all stock/patched differences and pin each backend's exact responses."""
import argparse
import gzip
import hashlib
import json
import re
from collections import Counter
from pathlib import Path

from extract import HERE, digest, line
from probes import rows

COMMENT = {"C_COMMENT", "SQL_COMMENT"}
TRACKED_END = {"IDENT", "UIDENT", "SCONST", "USCONST", "BCONST", "XCONST"}
PARAMETER_FORM = re.compile(
    r"^\s*(?:SELECT\s+(?:EXTRACT\s*\(|(?:DATE|TIMESTAMP|INTERVAL)\b)|"
    r"(?:CREATE|ALTER)\s+(?:ROLE|USER)\b.*?\bPASSWORD\s+|"
    r"SET\s+(?:SCHEMA|ROLE|SESSION AUTHORIZATION|search_path\s*=)\s+)\s*"
    r"(?:\(\d+\)\s*)?\$\d+",
    re.I | re.S,
)


def differences(case, stock, patched):
    tags = set()
    a, b = stock["tokens"], patched["tokens"]
    clean = [t for t in b if t["name"] not in COMMENT]
    if len(clean) != len(b):
        tags.add("04-comment-tokens")
    junk = stock["scan_error"] and "trailing junk after parameter" in stock["scan_error"]["message"]
    if junk and stock["scan_error"] != patched["scan_error"]:
        tags.add("09-parameter-junk")
    elif stock["scan_error"] != patched["scan_error"]:
        tags.add("unreviewed-scan-error")
    if len(a) != len(clean) and not junk:
        tags.add("unreviewed-token-count")
    for x, y in zip(a, clean):
        if (x["name"], x["start"]) != (y["name"], y["start"]):
            tags.add("unreviewed-token")
            break
        if x["id"] != y["id"]:
            tags.add("04-token-numbers" if x["id"] >= 275 and y["id"] == x["id"] + 2
                     else "unreviewed-token-number")
        if x["end"] != y["end"]:
            tags.add("03-token-end" if x["name"] in TRACKED_END else "unreviewed-token-end")
    if stock["parse_error"] != patched["parse_error"]:
        if junk:
            pass
        elif stock["parse_error"] and patched["parse_error"] is None and PARAMETER_FORM.search(case["sql"]):
            tags.add("01-parameter-references")
        else:
            tags.add("unreviewed-parse-error")
    if stock["tree"] and patched["tree"] and stock["tree"] != patched["tree"]:
        tree = re.sub(r" :(?:arg_location|payload_location|conninfo_location) -?\d+(?=[ }])", "", patched["tree"])
        if tree != patched["tree"]:
            tags.add("15-string-locations")
        limit = re.sub(r" :limitOption ([12])(?=[ }])", lambda m: " :limitOption " + str(int(m[1]) - 1), tree)
        if limit != tree:
            tags.add("05-limit-option")
        if limit != stock["tree"]:
            tags.add("unreviewed-tree")
    return sorted(tags)


def report(out):
    manifest = json.loads((out / "inventory.json").read_bytes())
    for filename, key in [("cases.jsonl", "cases_sha256"), ("skips.jsonl", "skips_sha256"),
                          ("sources.json", "sources_sha256")]:
        if digest((out / filename).read_bytes()) != manifest[key]:
            raise ValueError(f"{filename} differs from the extraction manifest")
    totals, sources, modes, groups = Counter(), Counter(), Counter(), Counter()
    states = {"stock": Counter(), "patched": Counter()}
    hashes = {flavor: hashlib.sha256() for flavor in states}
    with gzip.open(out / "outcomes.jsonl.gz", "rt", encoding="utf-8") as f, (out / "differences.jsonl").open("w", encoding="utf-8") as ledger:
        for case, result in zip(rows(out / "cases.jsonl"), map(json.loads, f), strict=True):
            if case["id"] != result["id"]:
                raise ValueError("case/outcome ID mismatch")
            for flavor, counts in states.items():
                outcome = result[flavor]
                error = outcome["parse_error"]
                counts[error["sqlstate"] if error else "accepted"] += 1
                hashes[flavor].update(line([case["id"], outcome]).encode())
                if case.get("origin") == "error_tests" and not error:
                    raise ValueError(f"expected fixture error: {case['id']} on {flavor}")
            tags = differences(case, result["stock"], result["patched"])
            if result["stock"] != result["patched"] and not tags:
                raise ValueError(f"unclassified difference: {case['id']}")
            if tags:
                ledger.write(line({"id": case["id"], "reasons": tags}))
            totals.update(tags)
            groups["different" if result["stock"] != result["patched"] else "identical"] += 1
            sources[case["family"]] += 1
            modes[case["mode"]] += 1
    patch_dir = HERE.parent / "patches"
    manifest.update({
        "overlay_sha256": digest("".join(line([p.name, digest(p.read_bytes())]) for p in sorted(patch_dir.glob("*.patch"))).encode()),
        "probe_source_sha256": digest((HERE.parent / "probe.c").read_bytes()),
        "families": sources, "modes": modes, "comparisons": groups,
        "parse_outcomes": states, "differences": totals,
        "differences_sha256": digest((out / "differences.jsonl").read_bytes()),
        "exact_outcomes_sha256": {f: h.hexdigest() for f, h in hashes.items()},
    })
    return json.dumps(manifest, sort_keys=True, indent=2) + "\n"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("out", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    summary = report(args.out)
    (args.out / "summary.json").write_text(summary)
    if args.check and summary != (HERE / "pg19.0-summary.json").read_text():
        raise SystemExit(f"reference corpus differs; review {args.out / 'summary.json'}")
    print(summary, end="")
