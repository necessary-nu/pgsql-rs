#!/usr/bin/env python3
"""Refresh Rust raw node declarations from an imported, patched PG19 build."""
import argparse
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parent
IDENTIFIER = r"[A-Za-z_][A-Za-z0-9_]*"
SCALARS = {
    "bool": "bool", "char": "u8", "int": "i32", "int16": "i16",
    "int32": "i32", "int64": "i64", "long": "i64", "uint32": "u32",
    "Oid": "u32", "Index": "u32", "SubTransactionId": "u32",
    "RelFileNumber": "u32", "AclMode": "u64", "Cardinality": "f64",
    "ParseLoc": "Location",
}
VALUES = {
    "A_Const": "AConst", "Integer": "i32", "Float": "String",
    "String": "String", "Boolean": "bool", "BitString": "String",
    "List": "NodeList", "IntList": "Vec<i32>", "OidList": "Vec<u32>",
    "XidList": "Vec<u32>",
}
RAW_KEYWORDS = {"as", "in", "match", "move", "ref", "override", "type", "where"}

def field_name(name):
    snake = re.sub(r"([A-Z]+)([A-Z][a-z])", r"\1_\2", name)
    snake = re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", snake).lower()
    return "r#" + snake if snake in RAW_KEYWORDS else snake

def declarations(source, build):
    include = source / "src/include"
    data = subprocess.check_output([
        "clang", "-Wno-ignored-attributes", "-D_GNU_SOURCE",
        "-I" + str(build / "src/include"), "-I" + str(include),
        "-include", "postgres.h", "-Xclang", "-ast-dump=json",
        "-fsyntax-only", "-x", "c", include / "nodes/parsenodes.h",
    ])
    return json.loads(data)["inner"]

def fields(record):
    return (field for field in record["inner"] if field["kind"] == "FieldDecl")

def base_type(field):
    t = field["type"]["qualType"]
    return t.removeprefix("const ").removeprefix("struct ").rstrip(" *")

def raw_records(source, records, tags):
    grammar = (source / "src/backend/parser/gram.y").read_text()
    grammar = re.sub(r"/\*.*?\*/", " ", grammar, flags=re.S)
    words = set(re.findall(IDENTIFIER, grammar))
    # Include nodes constructed by make* helpers, even when the grammar casts
    # their return values straight to Node*. Raw structs can embed base structs.
    roots = set(re.findall(r"typedef struct (\w+)", (source / "src/include/nodes/parsenodes.h").read_text()))
    roots.update(name for name in records if name in words or "make" + name in words)
    roots.update(re.findall(r"\bmakeNode\((\w+)\)", grammar))
    pending = list(roots & tags.keys() - VALUES.keys())
    selected = set()
    while pending:
        name = pending.pop()
        if name in selected:
            continue
        selected.add(name)
        for field in fields(records[name]):
            target = base_type(field)
            if target in tags and target not in VALUES and target != "Bitmapset":
                pending.append(target)
    return sorted(selected, key=tags.get)

def cyclic_edges(records, selected):
    # Box and embedded records can recurse without touching a tagged Node.
    # Mark each typed pointer on a cycle so a bare record is safe to drop, too.
    graph = {name: {base_type(f) for f in fields(records[name])} & set(selected)
             for name in selected}
    cyclic = set()
    for parent, children in graph.items():
        for child in children:
            pending, seen = [child], set()
            while pending:
                name = pending.pop()
                if name == parent:
                    cyclic.add((parent, child))
                    break
                if name not in seen:
                    seen.add(name)
                    pending.extend(graph[name] - seen)
    return cyclic


def rust_type(field, enums, selected):
    ctype = field["type"]["qualType"]
    base = base_type(field)
    if ctype in ("NodeTag", "Expr"):
        return None   # The enclosing Node variant carries the C tag.
    if ctype == "List *":
        return "Vec<Node>"
    if ctype in ("char *", "const char *"):
        return "Option<String>"
    if ctype.endswith("*") and base in VALUES:
        return f"Option<Box<{VALUES[base]}>>" if base == "A_Const" else f"Option<{VALUES[base]}>"
    if ctype == "Bitmapset *":
        return "Bitmapset"
    if ctype in ("Node *", "Expr *"):
        return "Option<Box<Node>>"
    if base in selected:
        name = base.replace("_", "")
        return f"Option<Box<{name}>>" if ctype.endswith("*") else name
    if ctype in enums:
        return ctype
    if ctype in SCALARS:
        return SCALARS[ctype]
    raise ValueError(f"unmapped C field {field['name']}: {ctype}")

def emit_enum(name, enum):
    lines = [f"    {name.replace('_', '')} {{"]
    value = -1
    for item in enum["inner"]:
        if item["kind"] != "EnumConstantDecl":
            continue
        expressions = [child for child in item.get("inner", []) if child["kind"] == "ConstantExpr"]
        value = int(expressions[0]["value"]) if expressions else value + 1
        lines.append(f"        {item['name']} = {value},")
    return lines + ["    }"]

def equal_fields(build, selected, header):
    generated = (build / "src/backend/nodes/equalfuncs.funcs.c").read_text()
    result = [header + "compare_records! {"]
    unsupported = []
    for name in selected:
        native = name.replace("_", "")
        m = re.search(rf"^_equal{name}\([^\n]*\)\n\{{(.*?)^\}}", generated, re.M | re.S)
        if not m:
            if name not in {"CallContext", "InlineCodeBlock"}:
                raise ValueError(f"unrecognized PostgreSQL equality handler for {name}")
            unsupported.append(native)
            continue
        plain, edges = [], []
        for stmt in m[1].split(";"):
            stmt = stmt.strip()
            if stmt in ("return true", ""):
                continue
            call = re.fullmatch(r"COMPARE_(SCALAR|STRING|BITMAPSET|NODE|LOCATION)_FIELD\(([\w.]+)\)", stmt)
            if not call:
                raise ValueError(f"unrecognized equality operation for {name}: {stmt}")
            kind, field = call.groups()
            if kind != "LOCATION":
                target = edges if kind == "NODE" else plain
                target.append(".".join(field_name(part) for part in field.split(".")))
        result.append(f'    {native} {{ plain {{ {", ".join(plain)} }} edges {{ {", ".join(edges)} }} }}')
    result.extend([f'    @unsupported {{ {", ".join(unsupported)} }}', "}"])
    return result


def generate(run, check):
    source, build = run / "patched-source", run / "patched"
    tags_text = (build / "src/backend/nodes/nodetags.h").read_text()
    tags = {name: int(value) for name, value in re.findall(r"\bT_(\w+)\s*=\s*(\d+),", tags_text)}
    ast = declarations(source, build)
    records = {item["name"]: item for item in ast if item["kind"] == "RecordDecl" and item.get("completeDefinition") and item.get("name")}
    enums = {item["name"]: item for item in ast if item["kind"] == "EnumDecl" and item.get("name") and item.get("inner")}
    selected = raw_records(source, records, tags)
    header = "// Generated from the patched PostgreSQL 19.0 node headers by generate.py. See COPYRIGHT.postgresql.\n"
    nodes = [header + "node_types! {", "    values {"]
    for name, payload in VALUES.items():
        nodes.append(f'        {name.replace("_", "")} ({tags[name]}, "{name}", {payload})')
    nodes.extend(["    }", "    records {"])
    used_enums = set()
    cyclic = cyclic_edges(records, selected)
    edges = [header + "record_edges! {"]
    for name in selected:
        nodes.append(f'        {name.replace("_", "")} ({tags[name]}, "{name}") {{')
        children = []
        plain = []
        for field in fields(records[name]):
            t = rust_type(field, enums, selected)
            if t is None:
                continue
            cname = field["name"]
            target = base_type(field)
            if (name, target) in cyclic and t.startswith("Option<Box<"):
                t = t.replace("Option<Box<", "Option<NodeBox<", 1)
            if t == "Vec<Node>" or target in selected or t == "Option<Box<Node>>":
                children.append(field_name(cname))
            else:
                plain.append(field_name(cname))
            if name == "MultiAssignRef" and cname == "source":
                # PG shares one RHS expression among all assignment targets.
                t = "Option<Arc<Node>>"
            if name == "RawStmt" and cname == "stmt_len":
                t = "i32"  # ParseLoc in C, but this field stores a length.
            if t in enums:
                used_enums.add(t)
                t = t.replace("_", "")
            nodes.append(f'            {field_name(cname)} ["{cname}"]: {t},')
        nodes.append("        }")
        edges.append(f'    {name.replace("_", "")} {{ {", ".join(children)} }} {{ {", ".join(plain)} }}')
    edges.append("}")
    nodes.extend(["    }", "}"])
    enum_lines = [header + "pg_enums! {"]
    for name in sorted(used_enums):
        enum_lines.extend(emit_enum(name, enums[name]))
    enum_lines.append("}")
    for path, lines in [("nodes.in", nodes), ("enums.in", enum_lines), ("edges.in", edges), ("equal.in", equal_fields(build, selected, header))]:
        dest = ROOT / "src" / path
        contents = "\n".join(lines) + "\n"
        if check:
            if dest.read_text() != contents:
                raise SystemExit(f"generated definitions differ: {dest}")
        else:
            dest.write_text(contents)

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path, help="completed pg19-reference import directory")
    parser.add_argument("--check", action="store_true", help="verify the checked-in files")
    args = parser.parse_args()
    generate(args.run.resolve(), args.check)
