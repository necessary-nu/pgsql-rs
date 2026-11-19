"""Build the library summary operation against a completed PostgreSQL 19 import."""
import argparse
import re
import subprocess
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def build(run, work):
    work.mkdir(parents=True, exist_ok=True)
    header = re.sub(r"^#define (?:PG_MAJORVERSION|PG_VERSION|PG_VERSION_NUM) .+$", "", (ROOT / "pg_query.h").read_text(), flags=re.M)
    (work / "pg_query.h").write_text(header)
    build = run / "patched/src"
    tags = set(re.findall(r"\bT_(\w+)", (build / "include/nodes/nodetags.h").read_text()))
    source = (ROOT / "src/pg_query_summary_statement_type.c").read_text()
    # PG19 removes some planner/executor-only tags from the catch-all group.
    handler, unused = source.split("All the cases below here", maxsplit=1)
    old = 'case T_ClusterStmt:\n\t\t\tadd_stmt_type_if_needed(summary, "ClusterStmt");'
    if handler.count(old) != 1 or not {"RepackStmt", "WaitStmt"} <= tags:
        raise ValueError("unexpected PG19 CLUSTER/REPACK/WAIT statement schema")
    handler = handler.replace(old, 'case T_RepackStmt:\n\t\t\tadd_stmt_type_if_needed(summary, "RepackStmt");\n\t\t\tbreak;\n\t\tcase T_WaitStmt:\n\t\t\tadd_stmt_type_if_needed(summary, "WaitStmt");')
    for tag in re.findall(r"case T_(\w+):", handler):
        if tag not in tags:
            raise ValueError(f"PG19 lacks a summary statement handler's tag: {tag}")
    unused = re.sub(r"^\s*case T_(\w+):$", lambda m: m[0] if m[1] in tags else "", unused, flags=re.M)
    (work / "statements.c").write_text(handler + "All the cases below here" + unused)
    subprocess.run(["make", "-s", "-j4", "-C", build / "backend", "-f", "Makefile", "-f",
                    HERE / "summary-probe.mk", f"repo={ROOT}", f"work={work}", work / "summary-probe"], check=True)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("run", type=Path)
    p.add_argument("out", type=Path)
    args = p.parse_args()
    build(args.run.resolve(), args.out.resolve())
