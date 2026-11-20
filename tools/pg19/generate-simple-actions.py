"""Translate single-assignment gram.y actions whose C operations have native equivalents."""
import argparse
from pathlib import Path
import re
import runpy
import tempfile


TOOLS = Path(__file__).resolve().parent
CRATES = TOOLS.parents[1] / "crates"
TABLES = runpy.run_path(str(TOOLS / "generate-tables.py"))
SCALARS = set("ival chr boolean jtype dbehavior oncommit objtype fun_param_mode mergematch retoptionkind".split())


def call(expr):
    found = re.fullmatch(r"(\w+)\((.*)\)", expr)
    if not found:
        return None, []
    name, args = found.groups()
    parts = re.split(r',(?=(?:[^()"]|"[^"]*"|\([^()]*\))*$)', args)
    return name, [part.strip() for part in parts]


class Untranslated(Exception):
    pass


class Action:
    def __init__(self, constants, records, pointers):
        self.constants = constants
        self.records = records
        self.pointers = pointers

    def scalar(self, expr):
        if expr in self.constants:
            return str(self.constants[expr])
        if re.fullmatch(r"-?[0-9]{1,9}", expr):
            return str(int(expr))
        if found := re.fullmatch(r"\$(\d+):(?:ival|chr|boolean)", expr):
            return f"rhs.take({found[1]}).integer()?"
        if found := re.fullmatch(r"'([a-zA-Z0-9_ ])'", expr):
            return str(ord(found[1]))
        raise Untranslated

    def string(self, expr):
        if found := re.fullmatch(r"\$(\d+):(?:str|keyword)", expr):
            return f"rhs.take({found[1]}).text()?"
        if re.fullmatch(r'"[A-Za-z0-9_ .:+*/<>=!-]*"', expr):
            return f"crate::raw::String::from_static({expr})"
        name, args = call(expr)
        if name == "pstrdup" and len(args) == 1:
            return self.string(args[0])
        raise Untranslated

    def node(self, expr):
        if expr.startswith("(Node *)"):
            return self.node(expr[8:].strip())
        if expr in ("NULL", "NIL"):
            return "Node::Null"
        if found := re.fullmatch(r"\$(\d+):(\w+)", expr):
            if found[2] in self.pointers:
                return f"rhs.take({found[1]}).cell()?"
        name, args = call(expr)
        if len(args) == 1:
            if name in ("makeString", "makeFloat", "makeBitString"):
                return f"Node::{name[4:]}({self.string(args[0])})"
            if name == "makeInteger":
                return f"Node::Integer({self.scalar(args[0])})"
            if name == "makeBoolean" and args[0] in ("false", "true"):
                return f"Node::Boolean({args[0]})"
            if name == "makeNode" and args[0] in self.records:
                return f"crate::raw::{self.records[args[0]]}::default().into()"
        if len(args) == 2 and name in ("makeIntConst", "makeStringConst", "makeFloatConst", "makeBitStringConst", "makeBoolAConst"):
            tag = {"makeIntConst": "Integer", "makeStringConst": "String", "makeFloatConst": "Float",
                   "makeBitStringConst": "BitString", "makeBoolAConst": "Boolean"}[name]
            if not re.fullmatch(r"@\d+", args[1]):
                raise Untranslated
            if tag == "Integer":
                val = self.scalar(args[0])
            elif tag == "Boolean":
                if args[0] not in ("true", "false"):
                    raise Untranslated
                val = args[0]
            else:
                val = self.string(args[0])
            return (f"crate::raw::AConst {{ val: crate::raw::Literal::{tag}({val}), "
                    f"location: rhs.location({args[1][1:]}) }}.into()")
        raise Untranslated

    def translate(self, kind, expr):
        if found := re.fullmatch(r"\$(\d+):(\w+)", expr):
            if found[2] == kind:
                return f"Take({found[1]})"
        if expr in ("NIL", "NULL"):
            if kind == "list":
                return "Nil"
            if kind in self.pointers or kind == "str":
                return "Empty"
        if kind == "str":
            body = f"Value::from_text({self.string(expr)})"
        elif kind in SCALARS:
            body = f"Value::Integer({self.scalar(expr)})"
        elif kind == "list":
            name, args = call(expr)
            if name == f"list_make{len(args)}" and 1 <= len(args) <= 5:
                body = "Value::from_list(vec![" + ", ".join(self.node(arg) for arg in args) + "])"
            elif name == "lappend" and len(args) == 2 and (found := re.fullmatch(r"\$(\d+):list", args[0])):
                body = f"{{ let mut list = rhs.take({found[1]}).list()?; list.push({self.node(args[1])}); Value::from_list(list) }}"
            elif name == "list_concat" and len(args) == 2 and all(re.fullmatch(r"\$\d+:list", arg) for arg in args):
                a, b = (arg[1:].split(":")[0] for arg in args)
                body = f"{{ let mut list = rhs.take({a}).list()?; list.extend(rhs.take({b}).list()?); Value::from_list(list) }}"
            else:
                raise Untranslated
        elif kind in self.pointers:
            source = expr.removeprefix("(Node *)").strip()
            if found := re.fullmatch(r"\$(\d+):(\w+)", source):
                if found[2] not in self.pointers or found[2] == "list":
                    raise Untranslated
                return f"Take({found[1]})"
            body = f"Value::from_node({self.node(expr)})"
        else:
            raise Untranslated
        arg = "rhs" if "rhs." in body else "_"
        return f"Generated(|{arg}| Ok({body}))"


def generate(source, work):
    gram = source / "src/backend/parser/gram.y"
    raw, grammar, c = TABLES["gram_report"](gram, work)
    arrays = {key: TABLES["c_array"](c, key) for key in ("yyr1", "yyr2")}
    rows, _ = TABLES["inventory"](grammar, c, arrays)
    bodies = TABLES["action_bodies"](c)
    nodes = (CRATES / "pg19-nodes/src/nodes.in").read_text()
    records = {cname: rust for rust, cname in re.findall(r'^\s+(\w+) \(\d+, "([^"]+)"\) \{', nodes, re.M)}
    union = raw.decode().split("%union", 1)[1].split("}", 1)[0]
    pointers = {"node", "list"} | {field for typ, field in re.findall(r"(\w+)\s+\*(\w+);", union) if typ in records}
    constants = {"true": 1, "false": 0}
    enums = (CRATES / "pg19-nodes/src/enums.in").read_text()
    constants.update((key, int(value)) for key, value in re.findall(r"^\s+(\w+) = (-?\d+),", enums, re.M))
    for header in ("catalog/pg_class.h", "catalog/pg_attribute.h", "nodes/parsenodes.h"):
        text = (source / "src/include" / header).read_text()
        constants.update((name, ord(value)) for name, value in
                         re.findall(r"^#define\s+(\w+)\s+'([a-zA-Z0-9 ])'", text, re.M))
    translator = Action(constants, records, pointers)
    output = ["# Generated from the patched PG19 grammar by tools/pg19/generate-simple-actions.py.\n"]
    for number, (_, body) in bodies.items():
        sig, _, digest = rows[number - 1].rstrip().split("\t")
        arg_count = arrays["yyr2"][number]
        # Only erase whole C comments outside string and character literals.
        body = re.sub(r'''("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*')|/\*.*?\*/''',
                      lambda m: m[1] or " ", body, flags=re.S)
        match = re.fullmatch(r"\s*\{\s*\(yyval\.(\w+)\)\s*=\s*([^;]+);\s*\}\s*", body, re.S)
        if not match:
            continue
        kind, expr = match.groups()
        offsets = [int(offset) for offset in re.findall(r"\(yyvsp\[(-?\d+)\]\.", expr)]
        # Taking a semantic value moves it. References outside this RHS are live
        # in the parent, and using one value twice would require a typed copy.
        if len(set(offsets)) != len(offsets) or any(not 1 <= arg_count + n <= arg_count for n in offsets):
            continue
        expr = re.sub(r"\(yyvsp\[(-?\d+)\]\.(\w+)\)", lambda m: f"${arg_count + int(m[1])}:{m[2]}", expr)
        expr = re.sub(r"\(yylsp\[(-?\d+)\]\)", lambda m: f"@{arg_count + int(m[1])}", expr)
        try:
            op = translator.translate(kind, expr.strip())
        except Untranslated:
            continue
        output.append(f"{sig}\t{digest}\t{op}\n")
    return "".join(output)


if __name__ == "__main__":
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("source", type=Path)
    cli.add_argument("--write", action="store_true")
    args = cli.parse_args()
    with tempfile.TemporaryDirectory() as temp:
        content = generate(args.source, Path(temp))
    path = CRATES / "pg19-parser/simple.tsv"
    if args.write:
        path.write_text(content)
    elif path.read_text() != content:
        cli.exit(1, f"regeneration differs for {path}\n")
