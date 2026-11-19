#!/usr/bin/env python3
"""Lower PostgreSQL record constructors and utility actions into typed Rust.

Run with the patched PostgreSQL build directory. Unrecognized C remains unmapped,
including any constructor containing an unhandled field, call, or control flow.
"""
import argparse
import hashlib
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PARSER = ROOT / "crates/pg19-parser"
NODES = ROOT / "crates/pg19-nodes/src"
POINTER = re.compile(r"Option<Box<(\w+)>>")


class Unsupported(Exception):
    """A source action needs a dedicated translation."""


def node_fields():
    data = (NODES / "nodes.in").read_text()
    structs = {}
    for name, c_name, fields in re.findall(r'        (\w+) \(\d+, "(\w+)"\) \{(.*?)\n        \}', data, re.S):
        fields = re.findall(r'\s*(\w+|r#\w+) \["(\w+)"\]: (.*?),\n', fields + "\n")
        structs[c_name] = name, {c: (field, ty) for field, c, ty in fields}
    return structs


RECORDS = node_fields()
ENUMS = {}
for enum, members in re.findall(r"    (\w+) \{([^{}]+)\}", (NODES / "enums.in").read_text()):
    for member in re.findall(r"(\w+) = ", members):
        ENUMS[member] = enum
ENUM_TYPES = set(ENUMS.values())
CONSTANTS = {"InvalidOid": "0", "InvalidRelFileNumber": "0", "InvalidSubTransactionId": "0", "RELPERSISTENCE_PERMANENT": "i32::from(b'p')",
             "RELPERSISTENCE_UNLOGGED": "i32::from(b'u')", "RELPERSISTENCE_TEMP": "i32::from(b't')"}


def convert(value, ty):
    if ty == "Value":
        return value
    if ty == "Node":
        return f"{value}.cell()?"
    if ty == "Option<Box<Node>>":
        return f"optional_node({value})?"
    if ty == "Option<String>":
        return f"optional_text({value})?"
    if ty == "String":
        return f"{value}.text()?"
    if ty == "Vec<Node>":
        return f"{value}.list()?"
    if ty == "i32":
        return f"{value}.integer()?"
    if ty == "bool":
        return f"{value}.integer()? != 0"
    if ty in ("u8", "i16", "u16", "u32", "i64", "u64"):
        return f"({value}.integer()?) as {ty}"
    if ty in ENUM_TYPES:
        return f"{ty}({value}.integer()?)"
    if m := POINTER.fullmatch(ty):
        return f"record!({value}, {m[1]})"
    raise Unsupported(ty)


def apply_member(record, assignment, variable):
    if not (m := re.fullmatch(rf"{variable}->(\w+) = (.*)", assignment)):
        raise Unsupported(assignment)
    field = RECORDS[record][1].get(m[1])
    if not field:
        raise Unsupported(assignment)
    native, ty = field
    return native, lower(m[2], ty)


def arguments(s):
    args = []
    depth = 0
    start = 0
    for i, token in enumerate(s):
        if token == "(":
            depth += 1
        elif token == ")":
            depth -= 1
        elif token == "," and depth == 0:
            args.append(s[start:i].strip())
            start = i + 1
    return args + [s[start:].strip()]


def lower(s, ty="Value"):
    def pointer_cast(match):
        # PG's Node/Expr and tagged-record pointers use the same semantic value.
        # Scalar fields and typed Rust record edges are checked by convert().
        if match[1] in RECORDS or match[1] in {"Node", "Expr", "List", "char"}:
            return ""
        return match[0]
    s = re.sub(r"\((\w+)\s*\*\)\s*", pointer_cast, s.strip())
    if m := re.fullmatch(r"\$(\d+)\.\w+", s):
        return convert(f"rhs.take({m[1]})", ty)
    if m := re.fullmatch(r"@(\d+)", s):
        if ty != "Location":
            raise Unsupported(s)
        return f"rhs.location({m[1]})"
    if ty == "Location" and s == "-1":
        return "Location::UNKNOWN"
    if s in ("NULL", "NIL"):
        if ty.startswith("Option<"):
            return "None"
        if ty == "Vec<Node>":
            return "Vec::new()"
        if ty == "Node":
            return "Node::Null"
        if ty == "Value":
            return "Value::List(Vec::new())" if s == "NIL" else "Value::Empty"
        raise Unsupported(s)
    if re.fullmatch(r'"[\w .:-]*"', s):
        result = s + ".to_owned()"
        if ty == "Option<String>":
            return f"Some({result})"
        if ty == "String":
            return result
        if ty == "Value":
            return f"Value::Text({result})"
        raise Unsupported(s)
    if re.fullmatch(r"\+\d+", s):
        s = s[1:]
    if s == "DEFAULT_INDEX_TYPE":
        return lower('"btree"', ty)
    if m := re.fullmatch(r"!\((\$\d+\.\w+)\)", s):
        s = "!" + m[1]
    if s.startswith("!") and ty == "bool":
        return f"{lower(s[1:], 'i32')} == 0"
    if s in ("true", "false") and ty == "bool":
        return s
    if s in ("true", "false"):
        s = "1" if s == "true" else "0"
    if s in ENUMS and ty == ENUMS[s]:
        return f"{ty}::{s}"
    integer = CONSTANTS.get(s)
    if s in ENUMS:
        integer = f"{ENUMS[s]}::{s}.0"
    elif re.fullmatch(r"-?\d+", s):
        integer = s
    elif re.fullmatch(r"'[a-zA-Z ]'", s):
        integer = f"i32::from(b{s})"
    if integer is not None:
        if ty == "Value":
            return f"Value::Integer({integer})"
        if ty in ("i32", "i16", "u8", "u16", "u32", "i64", "u64"):
            return integer if re.fullmatch(r"-?\d+", integer) or ty == "i32" else f"({integer}) as {ty}"
        raise Unsupported(s)
    if m := re.fullmatch(r"([+-])\s+(\$\d+\.ival)", s):
        value = lower(m[2], "i32")
        if m[1] == "-":
            value += ".wrapping_neg()"
        if ty == "Value":
            return f"Value::Integer({value})"
        raise Unsupported(s)
    if not (m := re.fullmatch(r"(\w+)\((.*)\)", s)):
        raise Unsupported(s)
    name, raw = m.groups()
    args = arguments(raw)
    if name == "makeRangeVarFromAnyName" and len(args) == 3 and args[2] == "yyscanner":
        value = f"any_relation({lower(args[0], 'Vec<Node>')}, {lower(args[1], 'Location')})?"
        if ty == "Option<Box<RangeVar>>":
            return f"Some(Box::new({value}))"
        return convert(f"Value::Node({value}.into())", ty)
    if name == "with_persistence" and len(args) == 2:
        return convert(f"persistence({lower(args[0])}, {lower(args[1], 'i32')})?", ty)
    if name == "pstrdup" and len(args) == 1:
        return lower(args[0], ty)
    if name == "castNode" and len(args) == 2 and args[0] in RECORDS:
        return lower(args[1], ty)
    if name == "NameListToString" and len(args) == 1:
        value = f"Value::Text(dotted_name({lower(args[0], 'Vec<Node>')})?)"
    elif name == "makeNode" and len(args) == 1 and args[0] in RECORDS:
        record = RECORDS[args[0]][0]
        if ty == f"Option<Box<{record}>>":
            return f"Some(Box::default())"
        value = f"Value::Node({record}::default().into())"
    elif name == "makeVacuumRelation" and len(args) == 3 and args[1] == "InvalidOid":
        value = f"VacuumRelation {{ relation: {lower(args[0], 'Option<Box<RangeVar>>')}, oid: 0, va_cols: {lower(args[2], 'Vec<Node>')} }}"
        if ty == "Option<Box<VacuumRelation>>":
            return f"Some(Box::new({value}))"
        value = f"Value::Node({value}.into())"
    elif name in ("makeDefElem", "makeDefElemExtended") and len(args) == (3 if name == "makeDefElem" else 5):
        if name == "makeDefElem":
            namespace, option, arg, action, loc = ["NULL", *args[:2], "DEFELEM_UNSPEC", args[2]]
        else:
            namespace, option, arg, action, loc = args
        record = (f"DefElem {{ defnamespace: {lower(namespace, 'Option<String>')}, defname: {lower(option, 'Option<String>')}, "
                  f"arg: {lower(arg, 'Option<Box<Node>>')}, defaction: {lower(action, 'DefElemAction')}, "
                  f"location: {lower(loc, 'Location')}, arg_location: Location::UNKNOWN }}")
        value = f"Value::Node({record}.into())"
    elif name in ("makeParamRef", "makeRoleSpec") and len(args) == 2:
        loc = lower(args[1], "Location")
        if name == "makeParamRef":
            rec = f"ParamRef {{ number: {lower(args[0], 'i32')}, location: {loc} }}"
        else:
            rec = f"RoleSpec {{ roletype: {lower(args[0], 'RoleSpecType')}, rolename: None, location: {loc} }}"
        value = f"Value::Node({rec}.into())"
    elif name in ("makeString", "makeInteger", "makeFloat", "makeBoolean") and len(args) == 1:
        variant, argtype = {"makeString": ("String", "String"), "makeInteger": ("Integer", "i32"),
                            "makeFloat": ("Float", "String"), "makeBoolean": ("Boolean", "bool")}[name]
        node = f"Node::{variant}({lower(args[0], argtype)})"
        if ty == "Node":
            return node
        if ty == "Option<Box<Node>>":
            return f"Some(Box::new({node}))"
        value = f"Value::Node({node})"
    elif name in ("makeStringConst", "makeIntConst", "makeFloatConst", "makeAConst") and len(args) == 2:
        loc = lower(args[1], "Location")
        if name == "makeAConst":
            literal = f"numeric_literal({lower(args[0], 'Node')})?"
        else:
            variant, argtype = {"makeStringConst": ("String", "String"), "makeIntConst": ("Integer", "i32"),
                                "makeFloatConst": ("Float", "String")}[name]
            literal = f"Literal::{variant}({lower(args[0], argtype)})"
        value = f"Value::Node(Node::AConst(AConst {{ val: {literal}, location: {loc} }}))"
    elif re.fullmatch(r"list_make[1-5]", name) and int(name[-1]) == len(args):
        items = ", ".join(lower(arg, "Node") for arg in args)
        if ty == "Vec<Node>":
            return f"vec![{items}]"
        value = f"Value::List(vec![{items}])"
    elif name in ("lappend", "lcons", "list_concat") and len(args) == 2:
        if name == "lcons":
            result = f"prepend({lower(args[0], 'Node')}, {lower(args[1], 'Vec<Node>')})"
        elif name == "lappend":
            result = f"append({lower(args[0], 'Vec<Node>')}, {lower(args[1], 'Node')})"
        else:
            result = f"concat({lower(args[0], 'Vec<Node>')}, {lower(args[1], 'Vec<Node>')})"
        if ty == "Vec<Node>":
            return result
        value = f"Value::List({result})"
    else:
        raise Unsupported(s)
    return convert(value, ty)


def constraint_properties(stmt, local):
    match = re.fullmatch(r"processCASbits\((.*), yyscanner\)", stmt)
    if not match:
        return None
    args = arguments(match[1])
    if len(args) != 8 or not re.fullmatch(r'"[A-Z ]+"', args[2]):
        raise Unsupported(stmt)
    targets = [("deferrable", "CAS_DEFERRABLE"), ("initdeferred", "CAS_INITIALLY_DEFERRED"),
               ("is_enforced", "CAS_ENFORCED"), ("skip_validation", "CAS_NOT_VALID"),
               ("is_no_inherit", "CAS_NO_INHERIT")]
    flags = []
    for dest, (field, flag) in zip(args[3:], targets):
        if dest == f"&{local}->{field}":
            flags.append(f"crate::utility::constraint::{flag}")
        elif dest != "NULL":
            raise Unsupported(stmt)
    mask = " | ".join(flags) or "0"
    return (f"crate::utility::constraint::process(&mut node, {lower(args[0], 'i32')}, "
            f"{lower(args[1], 'Location')}, {args[2]}, {mask})?;")


def nested_record(statements):
    """Translate a tree of fresh records, including writes after pointer assignment."""
    allocate = re.compile(r"(\w+)\s*\*\s*(\w+) = makeNode\((\w+)\)")
    setfield = re.compile(r"(\w+(?:->\w+)*)->(\w+) = (.+)")
    if not statements or not (root := allocate.fullmatch(statements[0])):
        raise Unsupported("not a record tree")
    if not re.fullmatch(rf"result\.\w+ = (?:\(Node \*\) )?{root[2]}", statements[-1]):
        raise Unsupported("not returning the outer record")
    records = {}
    parents = set()

    def add(local, c_name):
        if local in records or c_name not in RECORDS:
            raise Unsupported(local)
        records[local] = (c_name, {})

    for statement in statements[:-1]:
        if m := allocate.fullmatch(statement):
            if m[1] != m[3]:
                raise Unsupported(statement)
            add(m[2], m[1])
            continue
        m = setfield.fullmatch(statement)
        if not m or m[1] not in records:
            raise Unsupported(statement)
        owner, name, expr = m.groups()
        c_name, fields = records[owner]
        if name not in RECORDS[c_name][1] or name in fields:
            raise Unsupported(statement)
        if (created := re.fullmatch(r"makeNode\((\w+)\)", expr)):
            child = f"{owner}->{name}"
            add(child, created[1])
        else:
            child = re.sub(r"^\((?:Node|Expr) \*\) ", "", expr)
        if child in records:
            if child == owner or child in parents:
                raise Unsupported("shared record")
            parents.add(child)
            fields[name] = (child, None)
        else:
            fields[name] = (None, expr)
    if len(records) == 1 or set(records) - parents != {root[2]}:
        raise Unsupported("not a single tree of fresh records")

    visited = set()

    def init(local):
        visited.add(local)
        c_name, fields = records[local]
        native, domain = RECORDS[c_name]
        items = []
        for c_field, (child, expr) in fields.items():
            name, ty = domain[c_field]
            if child is None:
                rust = lower(expr, ty)
            else:
                desc = init(child)
                child_type = RECORDS[records[child][0]][0]
                if ty == f"Option<Box<{child_type}>>":
                    rust = f"Some(Box::new({desc}))"
                elif ty == "Option<Box<Node>>":
                    rust = f"Some(Box::new({desc}.into()))"
                elif ty == child_type:
                    rust = desc
                else:
                    raise Unsupported(ty)
            items.append(f"{name}: {rust}")
        if len(items) < len(domain):
            items.append("..Default::default()")
        return f"{native} {{ {', '.join(items)} }}"

    result = init(root[2])
    if visited != records.keys():
        raise Unsupported("unreachable local record")
    return f"Value::Node({result}.into())"


def compile_action(raw, symbols):
    body = re.sub(r"/\*.*?\*/", " ", raw.strip(), flags=re.S)
    if not (body.startswith("{") and body.endswith("}")):
        raise Unsupported(body)
    body = re.sub(r"\s+", " ", body[1:-1].strip())
    count = len(symbols.split(" %prec ")[0].split()) if symbols != "%empty" else 0
    body = re.sub(r"\(yyvsp\[(-?\d+)\]\.(\w+)\)", lambda m: f"${count + int(m[1])}.{m[2]}", body)
    body = re.sub(r"\(yylsp\[(-?\d+)\]\)", lambda m: f"@{count + int(m[1])}", body)
    body = re.sub(r"\(yyval\.(\w+)\)", lambda m: f"result.{m[1]}", body)
    # makeNode followed by assignments through $$ is the same allocation form.
    if m := re.fullmatch(r"result\.(\w+) = makeNode\((\w+)\); (result\.\1->.*;)", body):
        kind, rec, tail = m.groups()
        body = f"{rec} *n = makeNode({rec}); " + tail.replace(f"result.{kind}->", "n->") + f" result.{kind} = n;"
    # C mutates the owned RangeVar before moving it into the parent record.
    if m := re.search(r"(\$\d+\.range)->relpersistence = (\$\d+\.\w+); ", body):
        pointer, storage = m.groups()
        if body.count(pointer) != 2:
            raise Unsupported("shared relation")
        body = body[:m.start()] + body[m.end():].replace(pointer, f"with_persistence({pointer}, {storage})")
    body = re.sub(r"(\w+)\s*\*\s*(\w+) = castNode\(\1, (\$\d+\.\w+)\);",
                  lambda m: f"{m[1]} *{m[2]} = {m[3]};", body)
    # Ordinary views set a field on the relation after storing it on the view.
    body = re.sub(r"(\w+->\w+) = (\$\d+\.range); \1->relpersistence = (\$\d+\.\w+);",
                  lambda m: f"{m[1]} = with_persistence({m[2]}, {m[3]});", body)
    # Each stack value is owned. Multiple C reads require an explicit ownership translation.
    uses = re.findall(r"\$(\d+)\.", body)
    if len(uses) != len(set(uses)) or any(int(slot) < 1 for slot in uses):
        raise Unsupported("reused or inherited semantic value")
    statements = [s.strip() for s in body.split(";") if s.strip()]
    if not statements:
        raise Unsupported("empty body")
    # An assignment through a C pointer can modify an existing child record.
    # Its Rust owner must survive until the result has received those fields.
    if body.count("makeNode(") >= 2:
        try:
            return nested_record(statements)
        except Unsupported:
            pass
    existing = re.fullmatch(r"(\w+)\s*\*\s*(\w+) = (?:\(\w+ \*\) )?(\$\d+\.\w+)", statements[0])
    if existing:
        record, local, slot = existing.groups()
        if record not in RECORDS or not re.fullmatch(rf"result\.\w+ = (?:\(Node \*\) )?{local}", statements[-1]):
            raise Unsupported(statements[-1])
        changes = statements[1:-1]
    else:
        assigned = re.fullmatch(r"result\.(\w+) = (\$\d+\.\w+)", statements[0])
        kinds = {"ielem": "IndexElem", "fun_param": "FunctionParameter", "range": "RangeVar", "typnam": "TypeName",
                 "defelt": "DefElem", "constraint": "Constraint", "vsetstmt": "VariableSetStmt"}
        if assigned and assigned[1] in kinds and len(statements) > 1:
            record, local, slot = kinds[assigned[1]], "result." + assigned[1], assigned[2]
            changes = statements[1:]
        else:
            changes = None
    if changes:
        native = RECORDS[record][0]
        code = [f"let Node::{native}(mut node) = {lower(slot)}.node()? else {{ return Err(\"expected {native}\".into()); }};"]
        for stmt in changes:
            name, val = apply_member(record, stmt, re.escape(local))
            code.append(f"node.{name} = {val};")
        return "{ " + " ".join(code) + f" Value::Node(Node::{native}(node)) }}"
    if m := re.fullmatch(r"(\w+)\s*\*\s*(\w+) = makeNode\((\w+)\)", statements[0]):
        c_name, local, allocation = m.groups()
        if c_name != allocation or c_name not in RECORDS:
            raise Unsupported(c_name)
        record, fields = RECORDS[c_name]
        if not re.fullmatch(rf"result\.\w+ = (?:\(Node \*\) )?{local}", statements[-1]):
            raise Unsupported(statements[-1])
        assignments = []
        setup = []
        updates = []
        for stmt in statements[1:-1]:
            if c_name == "Constraint" and (properties := constraint_properties(stmt, local)) is not None:
                updates.append(properties)
                continue
            if updates:
                if stmt == f"{local}->initially_valid = !{local}->skip_validation":
                    updates.append("node.initially_valid = !node.skip_validation;")
                else:
                    key, expr = apply_member(c_name, stmt, re.escape(local))
                    updates.append(f"node.{key} = {expr};")
                continue
            split = re.fullmatch(rf"SplitColQualList\((\$\d+\.list), &{local}->constraints, &{local}->collClause, yyscanner\)", stmt)
            if c_name in ("ColumnDef", "CreateDomainStmt") and split:
                setup.append(f"let (constraints, coll_clause) = column_qualifiers({lower(split[1], 'Vec<Node>')})?;")
                assignments.extend([("constraints", "constraints"), ("coll_clause", "coll_clause")])
            else:
                assignments.append(apply_member(c_name, stmt, re.escape(local)))
        if len({field for field, _ in assignments}) != len(assignments):
            raise Unsupported("reassigned field")
        if not assignments:
            return f"Value::Node({record}::default().into())"
        init = ", ".join((key if key == val else f"{key}: {val}") for key, val in assignments)
        if len(assignments) != len(fields):
            init += ", ..Default::default()"
        constructor = f"{record} {{ {init} }}"
        if updates:
            setup.extend([f"let mut node = {constructor};", *updates])
            constructor = "node"
        value = f"Value::Node({constructor}.into())"
        return "{ " + " ".join(setup) + " " + value + " }" if setup else value
    if m := re.fullmatch(r"result\.(\w+) = (.*)", statements[0]):
        ctype, source = m.groups()
        if ctype == "list" and source == "NULL":
            source = "NIL"
        result = lower(source)
        if len(statements) == 1:
            return result
        if len(statements) == 2 and (m := re.fullmatch(r"result\.defelt->arg_location = (@\d+)", statements[1])):
            return f"with_arg_location({result}, {lower(m[1], 'Location')})?"
    raise Unsupported(body)


def generate(build):
    inventory = (ROOT / "crates/pg19-tables/data/productions.tsv").read_text().splitlines()
    source = (build / "src/backend/parser/gram.c").read_text()
    makefile = (build / "src/Makefile.global").read_text()
    source_tree = re.search(r"^abs_top_srcdir\s*=\s*(.+)$", makefile, re.M)
    if not source_tree:
        raise ValueError("PostgreSQL Makefile.global has no configured source directory")
    lock_header = Path(source_tree[1].strip()) / "src/include/storage/lockdefs.h"
    for name, value in re.findall(r"^#define\s+(\w+Lock)\s+(\d+)\b", lock_header.read_text(), re.M):
        CONSTANTS[name] = value
    pattern = (r"^  case (\d+): /\* [^\n]+ \*/\n#line \d+ \"[^\n]*gram.y\"\n"
               r"(.*?)\n#line \d+ \"gram.c\"\n    break;")
    simpler = {row.split("\t")[0] for row in (PARSER / "simple.tsv").read_text().splitlines()}
    manifest = ["# Generated from the pinned PG19 actions by generate-record-actions.py."]
    functions = {}
    for number, raw in re.findall(pattern, source, re.M | re.S):
        sig, _, digest = inventory[int(number) - 1].split("\t")
        _, symbols = sig.split(": ", 1)
        if sig in simpler:
            continue
        if hashlib.sha256(raw.strip().encode()).hexdigest() != digest:
            raise ValueError(f"grammar build differs from the node inventory at {sig}")
        try:
            code = compile_action(raw, symbols)
        except Unsupported:
            continue
        if m := re.fullmatch(r"rhs.take\((\d+)\)", code):
            op = f"Take({m[1]})"
        elif code == "Value::Empty":
            op = "Empty"
        elif code == "Value::List(Vec::new())":
            op = "Nil"
        else:
            index = functions.setdefault(code, (len(functions), []))[0]
            functions[code][1].append(sig)
            op = f"Generated(crate::records::ACTIONS[{index}])"
        manifest.append(f"{sig}\t{digest}\t{op}")
    out = ["// Generated by tools/pg19/generate-record-actions.py from PostgreSQL gram.y.",
           "pub(super) const ACTIONS: &[Action] = &["]
    for body, (_, signatures) in functions.items():
        out.append(f"    // {signatures[0]}")
        result = body[:-1] if body.startswith("with_arg_location(") and body.endswith("?") else f"Ok({body})"
        out.append(f"    |{'rhs' if 'rhs.' in body else '_'}| {result},")
    out.append("];")
    return {"records.tsv": manifest, "src/records.in": out}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("build", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    for relative, lines in generate(args.build).items():
        path = PARSER / relative
        content = "\n".join(lines) + "\n"
        if relative.endswith(".in"):
            content = subprocess.run(["rustfmt", "--edition", "2021", "--emit", "stdout"],
                                     input=content, text=True, capture_output=True, check=True).stdout
        if args.check:
            if path.read_text() != content:
                raise SystemExit(f"stale generated file: {path}")
        else:
            path.write_text(content)
