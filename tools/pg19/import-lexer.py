"""Read the pinned PG19/Flex build and write the native lexer's data tables."""
import argparse
import hashlib
from pathlib import Path
import re
import struct
import tempfile

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "crates/pg19-lexer/data"

def initializer(text, name):
    return re.search(r"\b" + name + r"\[\d+\] =\s*\{(.*?)\};", text, re.S)[1]

def export_dfa(scanner, output):
    # Flex -CF uses relative offsets and verifies the input byte at each edge.
    # Enumerating reachable states lets the Rust scanner use dense indexes.
    pairs = [(int(a), int(b)) for a, b in re.findall(r"\{\s*(-?\d+),\s*(-?\d+)\s*\}", initializer(scanner, "yy_transition"))]
    starts = [int(n) for n in re.findall(r"yy_transition\[(\d+)\]", initializer(scanner, "yy_start_state_list"))][1::2]
    states = list(starts)
    indexes = {state: i + 1 for i, state in enumerate(states)}
    transitions = []
    for state in states:
        row = [0]  # Byte zero is reserved for end-of-input in the C API.
        for byte in range(1, 256):
            verify, offset = pairs[state + byte] if state + byte < len(pairs) else (0, 0)
            if byte != verify:
                row.append(0)
                continue
            dest = state + offset
            if dest not in indexes:
                states.append(dest)
                indexes[dest] = len(states)
            row.append(indexes[dest])
        transitions.append(row)
    # Bytes whose transitions agree in every state share a column.
    columns = list(zip(*transitions))
    unique = list(dict.fromkeys(columns))
    classes = bytes(unique.index(col) for col in columns)
    rows = list(zip(*unique))
    accept = bytes([0] + [pairs[state - 1][1] for state in states])
    (output / "classes.bin").write_bytes(classes)
    (output / "actions.bin").write_bytes(accept)
    (output / "edges.bin").write_bytes(b"".join(struct.pack(f"<{len(row)}H", *row) for row in rows))
    return len(unique)

def export(run, output):
    src = run / "patched-source/src"
    build = run / "patched/src/backend/parser"
    for name, expected in [
        ("scan.l", "3c02439c273f76393b051a9cdceb3d84634fcf51f6666a198b8fa7b2ce185dbe"),
        ("parser.c", "6456c41096afcae3af42ca2d376fbcecae2367c9d6eee80d959bf5fc4e5ee316"),
    ]:
        if hashlib.sha256((src / "backend/parser" / name).read_bytes()).hexdigest() != expected:
            raise ValueError(f"review Rust lexical actions for the changed {name}")
    scanner = (build / "scan.c").read_text()
    cols = export_dfa(scanner, output)
    tokens = [(name, int(value)) for name, value in re.findall(r"^    (\w+) = (\d+),?", (build / "gram.h").read_text(), re.M) if int(value) >= 256]
    assert [v for _, v in tokens] == list(range(256, tokens[-1][1] + 1))
    keywords = re.findall(r'^PG_KEYWORD\("(\w+)",\s*(\w+),\s*(\w+),', (src / "include/parser/kwlist.h").read_text(), re.M)
    categories = {"UNRESERVED_KEYWORD": "Unreserved", "COL_NAME_KEYWORD": "ColumnName", "TYPE_FUNC_NAME_KEYWORD": "TypeFunctionName", "RESERVED_KEYWORD": "Reserved"}
    lines = ["// Generated from PostgreSQL 19.0 by tools/pg19/import-lexer.py.",
             f"pub(super) const COLUMNS: usize = {cols};", "impl Kind {"]
    lines.extend(f"    pub const {name.upper()}: Self = Self({value});" for name, value in tokens)
    lines += ["}", "pub(super) static NAMES: &[&str] = &["]
    lines.extend(f'    "{name}",' for name, _ in tokens)
    lines += ["];", "pub(super) static KEYWORDS: &[(&str, Kind, KeywordKind)] = &["]
    lines.extend(f'    ("{word}", Kind::{symbol.upper()}, KeywordKind::{categories[cat]}),' for word, symbol, cat in keywords)
    lines += ["];"]
    (output / "catalog.rs").write_text("\n".join(lines) + "\n")

if __name__ == "__main__":
    args = argparse.ArgumentParser(description=__doc__)
    args.add_argument("run", type=Path)
    args.add_argument("--check", action="store_true",
                      help="verify checked-in tables without replacing them")
    options = args.parse_args()
    if options.check:
        with tempfile.TemporaryDirectory() as tmp:
            destination = Path(tmp)
            export(options.run, destination)
            changed = [p.name for p in sorted(destination.iterdir())
                       if not (OUTPUT / p.name).exists()
                       or p.read_bytes() != (OUTPUT / p.name).read_bytes()]
            if changed:
                args.exit(1, f"regenerated scanner files differ: {', '.join(changed)}\n")
    else:
        export(options.run, OUTPUT)
