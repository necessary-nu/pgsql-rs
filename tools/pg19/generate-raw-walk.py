#!/usr/bin/env python3
"""Import the child order of PostgreSQL's raw expression tree walker."""
import argparse
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "crates/pg19-operations/src/raw_walk.in"
# These utility records have custom visitors in libpg_query.
UTILITY = """DefElem RawStmt VariableSetStmt CopyStmt CallStmt ExplainStmt CreateRoleStmt
AlterRoleStmt DeclareCursorStmt CreateFunctionStmt DoStmt CreateSubscriptionStmt
AlterSubscriptionStmt CreateUserMappingStmt AlterUserMappingStmt NotifyStmt""".split()


def generate(source):
    model = (ROOT / "crates/pg19-nodes/src/nodes.in").read_text()
    records = {}
    for native, pg, fields in re.findall(r'        (\w+) \(\d+, "(\w+)"\) \{(.*?)\n        \}', model, re.S):
        records[pg] = native, {c: (key, ty) for key, c, ty in
                               re.findall(r'(\w+|r#\w+) \["(\w+)"\]: (.*?),', fields)}
    source = source.split("raw_expression_tree_walker_impl(Node *node,")[1].split("\n\t\tdefault:")[0]
    source = re.sub(r"/\*.*?\*/", "", source, flags=re.S)
    parts = re.split(r"case T_(\w+):", source)
    nodes = {pg: [] for pg in UTILITY + ["CaseWhen"]}
    for pg, body in zip(parts[1::2], parts[2::2]):
        if pg == "List":
            assert re.search(r"WALK\(\(Node \*\) lfirst\(temp\)\)", body)
            continue
        if pg == "JsonConstructorExpr":  # analyzed expression, absent from the raw node model
            assert pg not in records
            continue
        walk = re.findall(r"WALK\((?:\(\(\w+ \*\) node\)|\w+)->(\w+)\)", body)
        if len(walk) != body.count("WALK("):
            raise ValueError(f"unrecognized child expression in {pg}")
        if pg == "CaseExpr":
            assert walk == ["arg", "expr", "result", "defresult"] and "foreach(temp, caseexpr->args)" in body
            walk = ["arg", "args", "defresult"]
            nodes["CaseWhen"] = ["expr", "result"]
        if pg in records:
            nodes[pg] = walk
        elif pg not in {"Integer", "Float", "Boolean", "String", "BitString", "A_Const"} or walk:
            raise ValueError(f"unknown raw record {pg}")
    rows = ["// Generated from PostgreSQL nodeFuncs.c by tools/pg19/generate-raw-walk.py.",
            "raw_records! {"]
    for pg, fields in sorted(nodes.items()):
        native, members = records[pg]
        children = []
        for field in fields:
            key, ty = members[field]
            if ty == "Vec<Node>":
                children.append(f"list {key}")
            elif re.fullmatch(r"Option<(Arc|NodeBox|Box)<\w+>>", ty):
                children.append(f"ptr {key}")
            else:
                raise ValueError(f"unsupported {pg}.{field} type {ty}")
        rows.append(f"    {native} {{ {', '.join(children)} }}")
    return "\n".join(rows + ["}", ""])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path, help="PG19 import directory")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    code = generate((args.run / "patched-source/src/backend/nodes/nodeFuncs.c").read_text())
    if args.check:
        if OUTPUT.read_text() != code:
            parser.exit(1, f"{OUTPUT} differs from the PG19 raw walker\n")
    else:
        OUTPUT.write_text(code)
