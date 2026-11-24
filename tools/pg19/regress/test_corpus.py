"""Exercise the corpus extractor against its pinned psql scanner."""
import argparse
import subprocess
import tempfile
from pathlib import Path

from extract import HERE, fixture_rows, line, source_spans
from probes import capture, compare


def check_boundaries(helper):
    sql = b"""-- start
CREATE FUNCTION f() RETURNS int LANGUAGE SQL
BEGIN ATOMIC
  SELECT CASE WHEN true THEN 1 ELSE 2 END;
  SELECT 2;
END;
CREATE RULE r AS ON SELECT TO t DO (SELECT 1; SELECT 2);
SELECT $q$; -- \\g$q$, ';', "a;b" /* /* ; */ ; */;
SELECT :psqlvar;
SELECT 1 \\gset
\\echo SELECT 2;
COPY t FROM STDIN;
;SELECT fake;
\\.
\\copy t from stdin
SELECT fake_again;
\\.
SELECT 1\\; COPY t FROM STDIN\\; COPY t FROM STDIN;
a
\\.
b
\\.
SELECT FROM;
SELECT 'open"""
    spans = [(kind, sql[a:b]) for kind, a, b in source_spans(helper, sql)]
    statements = [text.strip() for kind, text in spans if kind == "sql"]
    assert len(statements) == 7, statements
    assert statements[0].startswith(b"-- start") and statements[0].endswith(b"END;")
    assert statements[1].endswith(b"(SELECT 1; SELECT 2);")
    assert statements[2].endswith(b'/* /* ; */ ; */;')
    assert statements[3:] == [b"SELECT 1", b"COPY t FROM STDIN;", b"SELECT FROM;", b"SELECT 'open"]
    assert [text for kind, text in spans if kind == "copy-data"] == [
        b";SELECT fake;\n\\.\n", b"SELECT fake_again;\n\\.\n", b"a\n\\.\n", b"b\n\\.\n",
    ]
    assert ("psql-variable", b"\nSELECT :psqlvar;") in spans
    assert any(kind == "psql-escape" for kind, _ in spans)
    assert list(source_spans(helper, b"select '\xff'")) == [("non-utf8-file", 0, 10)]
    assert list(source_spans(helper, b"x\x00y")) == [("nul-file", 0, 3)]


def check_fixture_decoder(run):
    src = br'''const char* tests[] = {
      "SELECT " /* concatenate */ "'a\\n'", "expected", NULL
    };'''
    row, = fixture_rows(Path("test/parse_tests.c"), src)
    assert row["sql"] == r"SELECT 'a\n'"
    assert src[row["start"]:row["end"]].endswith(br'''"'a\\n'"''')
    cases = list(fixture_rows(Path("parse_opts_tests.c"), (run / "libpg_query/test/parse_opts_tests.c").read_bytes()))
    assert {r["mode"] for r in cases} == {
        "default", "type-name", "plpgsql-expr", "plpgsql-assign1", "plpgsql-assign2", "plpgsql-assign3",
    }
    assert [r["legacy_option_bits"] for r in cases if r["kind"] != "sql"] == [96, 100]


def check_comparator(run, out):
    cases = [
        {"id": str(i), "sql": sql, "mode": mode, "backslash_quote": "safe_encoding"}
        for i, (sql, mode) in enumerate([("SELECT FROM", "default"), ("SET ROLE $1", "default"),
                                         ("v.x := 1", "plpgsql-assign2")])
    ]
    (out / "cases.jsonl").write_text("".join(map(line, cases)))
    capture(run, out, 2)
    assert compare(run / "patched/probe", out, "patched", 2)
    assert not compare(run / "stock/probe", out, "patched", 2)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp)
        helper = out / "boundaries"
        subprocess.run(["sh", HERE / "build-boundaries.sh", args.run, helper], check=True)
        check_boundaries(helper)
        check_fixture_decoder(args.run)
        check_comparator(args.run, out)
