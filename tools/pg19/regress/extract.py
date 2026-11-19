"""Extract reproducible SQL cases from PostgreSQL and libpg_query source files."""
import ast
import hashlib
import json
import re
import subprocess
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
RELEASE = "postgresql-19.0"
ARCHIVE_SHA256 = "a89fd70efbad6a08c7dbed8d4872e3af30170d9e5ca9dc7906523be95b4e692d"
MODES = ("default", "type-name", "plpgsql-expr", "plpgsql-assign1", "plpgsql-assign2", "plpgsql-assign3")
C_TOKEN = re.compile(rb'\s+|//[^\n]*|/\*.*?\*/|"(?:\\.|[^"\\])*"|NULL|,|};', re.S)
C_ARRAY = re.compile(rb'const char\* (?:const )?(\w+)\[\] = \{')


def digest(data):
    return hashlib.sha256(data).hexdigest()


def line(row):
    return json.dumps(row, ensure_ascii=False, separators=(",", ":"), sort_keys=True) + "\n"


def source_spans(helper, data):
    # The probes accept UTF-8 C strings. A file with an unknown encoding can
    # contain ASCII-looking bytes inside multibyte characters.
    try:
        data.decode()
    except UnicodeDecodeError:
        yield "non-utf8-file", 0, len(data)
        return
    if b"\0" in data:
        yield "nul-file", 0, len(data)
        return
    events = subprocess.check_output([helper], input=data).splitlines()
    previous = 0
    for event in events:
        kind, start, end = event.decode("ascii").split()
        start, end = int(start), int(end)
        if not previous == start < end <= len(data):
            raise ValueError(f"noncontiguous boundary {event!r} after {previous}")
        yield kind, start, end
        previous = end
    if previous != len(data):
        raise ValueError("boundary scanner did not consume the whole file")


def c_arrays(data):
    for array in C_ARRAY.finditer(data):
        pos = array.end()
        fields, value, first, end = [], "", None, None
        while True:
            tok = C_TOKEN.match(data, pos)
            if not tok:
                raise ValueError(f"unsupported C fixture syntax at byte {pos}")
            pos = tok.end()
            text = tok.group()
            if text in (b"NULL", b"};"):
                if first is not None:
                    fields.append((first, end, value))
                break
            if text.startswith(b'"'):
                if first is None:
                    first = tok.start()
                end = pos
                value += ast.literal_eval(text.decode())
            elif text == b",":
                if first is None:
                    raise ValueError(f"empty C initializer at byte {pos}")
                fields.append((first, end, value))
                first, value = None, ""
        yield array[1].decode(), fields


def fixture_rows(path, data):
    arity = 3 if path.name in ("split_tests.c", "parse_opts_tests.c") else 2
    for array, fields in c_arrays(data):
        if len(fields) % arity:
            raise ValueError(f"incomplete fixture in {path}:{array}")
        for index in range(0, len(fields), arity):
            start, end, sql = fields[index]
            bits = int(fields[index + 1][2]) if path.name == "parse_opts_tests.c" else 0
            yield {
                "start": start, "end": end, "sql": sql, "mode": MODES[bits & 15],
                "backslash_quote": "off" if bits & 16 else "safe_encoding",
                "legacy_option_bits": bits, "origin": array,
                "kind": "legacy-string-settings" if bits & ~31 else "sql",
            }


def inventory(run, out):
    archive = run / (RELEASE + ".tar.bz2")
    if digest(archive.read_bytes()) != ARCHIVE_SHA256:
        raise ValueError("PostgreSQL release checksum mismatch")
    helper = out / "boundaries"
    subprocess.run(["sh", HERE / "build-boundaries.sh", run, helper], check=True)
    root = run / "stock-source"
    counts = Counter()
    inputs = {}
    with (out / "cases.jsonl").open("w", encoding="utf-8") as cases, (out / "skips.jsonl").open("w", encoding="utf-8") as skips:
        def record(family, path, data, row):
            start, end = row["start"], row["end"]
            row.update(id=f"{family}:{path}:{start}-{end}", family=family, path=str(path))
            if row["kind"] == "sql":
                cases.write(line(row))
            else:
                row["bytes_sha256"] = digest(data[start:end])
                skips.write(line(row))
            counts[row["kind"]] += 1

        for path in sorted((root / "src/test/regress/sql").glob("*.sql")):
            data = path.read_bytes()
            inputs["postgres:" + str(path.relative_to(root))] = digest(data)
            for kind, start, end in source_spans(helper, data):
                row = {"kind": kind, "start": start, "end": end}
                if kind == "sql":
                    row.update(sql=data[start:end].decode(), mode="default", backslash_quote="safe_encoding")
                record("postgres", path.relative_to(root), data, row)

        for name in ("parse_tests.c", "parse_opts_tests.c", "scan_tests.c", "split_tests.c"):
            path = Path("test", name)
            data = (REPO / path).read_bytes()
            inputs[f"libpg_query:{path}"] = digest(data)
            for row in fixture_rows(path, data):
                record("libpg_query", path, data, row)

        fixtures = HERE.parent / "fixtures"
        for entry in (fixtures / "cases.txt").read_text().splitlines():
            name, *flags = entry.split()
            opts = dict(zip(flags[::2], flags[1::2], strict=True))
            path = fixtures / (name + ".sql")
            data = path.read_bytes()
            inputs["smoke:" + str(path.relative_to(REPO))] = digest(data)
            record("smoke", path.relative_to(REPO), data, {
                "kind": "sql", "start": 0, "end": len(data), "sql": data.decode(),
                "mode": opts.get("--mode", "default"),
                "backslash_quote": opts.get("--backslash-quote", "safe_encoding"),
            })
    (out / "sources.json").write_text(json.dumps(inputs, sort_keys=True, indent=2) + "\n")
    return {"release": RELEASE, "archive_sha256": ARCHIVE_SHA256,
            "psqlscan_sha256": digest((root / "src/fe_utils/psqlscan.l").read_bytes()),
            "sources_sha256": digest((out / "sources.json").read_bytes()),
            "cases_sha256": digest((out / "cases.jsonl").read_bytes()),
            "skips_sha256": digest((out / "skips.jsonl").read_bytes()),
            "sources": len(inputs), "regions": dict(sorted(counts.items()))}


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path)
    parser.add_argument("out", type=Path)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    result = inventory(args.run.resolve(), args.out.resolve())
    (args.out / "inventory.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
