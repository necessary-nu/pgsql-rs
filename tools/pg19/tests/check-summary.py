"""Capture or check the PG19 summary wire result for every original summary test input."""
import argparse
import ast
import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SNAPSHOT = ROOT / "crates/pgsql-operations/tests/summary.jsonl"
STRING = r'"(?:\\.|[^"\\])*"'
LITERALS = rf'{STRING}(?:\s*{STRING})*'


def c_string(expr):
    return "".join(ast.literal_eval(m[0]) for m in re.finditer(STRING, expr))


def original_inputs(upstream):
    # Backslash-newline splicing precedes C tokenization. These tests use only
    # string literals or local names assigned concatenated string literals.
    code = (upstream / "test/summary_tests.c").read_text().replace("\\\n", "")
    functions = list(re.finditer(r"^void (\w+)\(TestState\* test_state\)", code, re.M))
    assignments = rf'\b(?P<var>sql|query)\s*=\s*(?P<value>{LITERALS})\s*;'
    calls = rf'\bsummary(?:_internal)?\(\s*(?P<arg>{LITERALS}|sql|query)\s*,\s*0\s*,\s*-1\s*\)'
    pattern = re.compile(f"{assignments}|{calls}")
    for i, function in enumerate(functions):
        body = code[function.end():functions[i+1].start() if i+1 < len(functions) else len(code)]
        body = re.sub(r"^\s*//.*$", "", body, flags=re.M)
        values = {}
        count = 0
        for m in pattern.finditer(body):
            if m["var"]:
                values[m["var"]] = c_string(m["value"])
            else:
                arg = m["arg"]
                sql = c_string(arg) if arg.startswith('"') else values[arg]
                yield f"{function[1]}/{count+1}", 0, sql
                count += 1
        if count != len(re.findall(r"\bsummary(?:_internal)?\(", body)):
            raise ValueError(f"unknown summary fixture syntax in {function[1]}")


def inputs(upstream):
    yield from original_inputs(upstream)
    yield from [
        ("qualified-function-and-filter", 0, "SELECT s.f(a) FROM s.t WHERE s.t.a = 1"),
        ("nested-filter-only", 0, "SELECT * FROM t WHERE a = (SELECT b FROM u WHERE c = 1)"),
        ("quoted-names-and-aliases", 0, 'SELECT "F"("É".x) FROM "both"."T" AS "É" WHERE "É"."?" > 1'),
        ("duplicate-alias", 0, "SELECT a.x FROM t a; SELECT a.y FROM u a"),
        ("multi-statement-types", 0, "EXPLAIN SELECT 1; PREPARE q AS INSERT INTO t VALUES (1); SELECT 2"),
        ("subquery-and-sibling-filter", 0, "SELECT * FROM t WHERE a IN (SELECT b FROM u WHERE c = 1) AND d = 2"),
        ("pg19-repack-and-wait", 0, "REPACK t USING INDEX ix; CLUSTER t; WAIT FOR LSN '0/1'"),
        ("empty", 0, ""),
        ("type-start", 1, "numeric(7,2)[]"),
        ("expression-start", 2, "f(a) FROM t WHERE a = 1"),
        ("assign-one", 3, "a := f(x) FROM t WHERE t.b > 2"),
        ("assign-two", 4, "a.b := f(x)"),
        ("assign-three", 5, "a.b.c := 1"),
    ]


def truncation_inputs(upstream):
    code = (upstream / "test/summary_truncate.c").read_text().replace("\\\n", "")
    functions = list(re.finditer(r"\bvoid\s+(\w+)\(TestState \* test_state\)", code))
    for i, function in enumerate(functions):
        body = code[function.end():functions[i+1].start() if i+1 < len(functions) else len(code)]
        query = re.search(rf'\bquery = ({LITERALS})\s*;', body)
        limit = int(re.search(r'summary\(query, 0, (\d+)\)', body)[1])
        assertion = re.search(rf'TEST_ASSERT_STR_EQUAL\(result.truncated_query, ({LITERALS}|output)\)', body)[1]
        if assertion == "output":
            assertion = re.search(rf'\boutput = ({LITERALS})\s*;', body)[1]
        yield function[1], 0, c_string(query[1]), limit, 0, c_string(assertion)
    for name, sql, limit in [
        ("empty-positive", "", 10),
        ("two-statements", "SELECT a,b,c,d; SELECT e,f,g,h", 27),
        ("multibyte-cutoff", 'SELECT * FROM "Åland"', 16),
        ("select-unchanged", " select f() from t -- comments", 100),
        ("zero-limit", "SELECT 1", 0),
        ("small-limit", "SELECT 1234", 2),
        ("negative-limit", "SELECT a,b,c,d", -2),
        ("delete-predicate", "DELETE FROM t WHERE abc=def AND ghi=1234567", 30),
        ("values-rows", "VALUES ('long words', 1), ('another long item', 234567)", 15),
        ("nested-query-filter", "SELECT foo FROM (SELECT abcdef FROM t WHERE reallylong = 15) s WHERE a = 145678", 58),
        ("conflict-predicate", "INSERT INTO t (a) VALUES (1) ON CONFLICT (a) WHERE abcdef = 12 DO UPDATE SET col = 0 WHERE lovely = 987654", 90),
        ("unique-covering-index", "CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS ix ON ONLY s.t USING btree (x DESC NULLS LAST, (a+b)) INCLUDE (q) NULLS NOT DISTINCT WITH (fillfactor=78) TABLESPACE testspace WHERE foo > 1", 1000),
        ("unnamed-expression-index", "CREATE INDEX ON t ((a + b), lower(c))", 1000),
        ("parameterized-opclass", "CREATE INDEX ix ON t (point gist_point_ops (siglen=4)) WITH (deduplicate_items=off)", 1000),
        ("conflict-opclass", "INSERT INTO t VALUES (1) ON CONFLICT (a gin_trgm_ops(siglen=8)) DO NOTHING", 1000),
        ("privileges-column-list", "GRANT SELECT (a, b), REFERENCES (c), UPDATE ON TABLE s.t TO PUBLIC, CURRENT_USER WITH GRANT OPTION", 1000),
        ("privileges-all-schema", "REVOKE GRANT OPTION FOR ALL ON ALL SEQUENCES IN SCHEMA s, t FROM foo CASCADE", 1000),
        ("privileges-routines", "GRANT EXECUTE ON FUNCTION f(integer), s.g(IN x text) TO SESSION_USER GRANTED BY CURRENT_ROLE", 1000),
        ("privileges-domain", "GRANT USAGE ON DOMAIN x.d, y.e TO PUBLIC", 1000),
        ("privileges-lobs", "GRANT ALL ON LARGE OBJECT 22, 345 TO bob", 1000),
        ("privileges-fdw", "GRANT USAGE ON FOREIGN DATA WRAPPER fdw TO u", 1000),
        ("privileges-sequences", "GRANT USAGE ON SEQUENCE a.b, c.d TO u", 1000),
        ("error-with-limit", "SELECT FROM )", 20),
        ("pg19-repack-wait", "REPACK t USING INDEX ix; WAIT FOR LSN '0/1'", 120),
        ("publication-all-sequences", "CREATE PUBLICATION p FOR ALL SEQUENCES", 100),
        ("publication-table-exceptions", 'CREATE PUBLICATION p FOR ALL SEQUENCES, ALL TABLES EXCEPT (TABLE ONLY s.t, "T two")', 160),
        ("publication-replace-objects", "ALTER PUBLICATION p SET ALL SEQUENCES, ALL TABLES", 100),
        ("publication-short-limit", 'ALTER PUBLICATION "Pub" SET ALL TABLES EXCEPT (TABLE a, b)', 42),
        ("subscription-foreign-server", 'CREATE SUBSCRIPTION "Sub" SERVER "Foreign Server" PUBLICATION p WITH (connect=false)', 120),
        ("subscription-change-server", 'ALTER SUBSCRIPTION s SERVER "Foreign Server"', 100),
        ("subscription-refresh-sequences", "ALTER SUBSCRIPTION s REFRESH SEQUENCES", 80),
        ("subscription-drop-cascade", 'DROP SUBSCRIPTION IF EXISTS "Sub" CASCADE', 90),
        ("stats-expressions", 'CREATE STATISTICS app.st (dependencies, mcv) ON "Col A", (lower(b)) FROM app.t', 100),
        ("stats-unnamed", "CREATE STATISTICS ON a, (b+c) FROM t", 80),
        ("stats-short-limit", 'CREATE STATISTICS "été" ON "Col A", b FROM app.t', 23),
        ("alter-stats-target", "ALTER STATISTICS IF EXISTS app.st SET STATISTICS DEFAULT", 90),
        ("database-refresh-collation", 'ALTER DATABASE "my db" REFRESH COLLATION VERSION', 80),
        ("database-option-grammar", '''CREATE DATABASE db "verbose" = 'some value' extra_option = -1.25''', 100),
        ("maintenance-option-grammar", 'VACUUM (ANALYZE, "select" true, "Case" 3.5) ONLY app.t(a)', 100),
        ("tablespace-quoted", 'ALTER TABLE ALL IN TABLESPACE "disk A" SET TABLESPACE "disk B"; DROP TABLESPACE "disk A"', 120),
        ("extension-dependency", 'ALTER INDEX app.i DEPENDS ON EXTENSION "my Éxt"', 80),
        ("publication-custom-options", 'CREATE PUBLICATION p WITH ("Select" = OPERATOR(pg_catalog.+), extarg = -2.50)', 120),
        ("tablespace-namespaced-option", """CREATE TABLESPACE ts LOCATION '/tmp' WITH ("select"."Ext Opt" = -2.50)""", 120),
        ("stats-custom-kind-and-join", 'CREATE STATISTICS app."my Stats" ("Custom Kind") ON lower(t.a), "my Col" FROM app.t t JOIN other u USING (a)', 160),
    ]:
        yield name, 0, sql, limit, 0, None
    yield "expression-with-limit", 2, "1+2", 20, 0, "SELECT 1 + 2"
    yield "escaped-quote-allowed", 0, r"SELECT E'it\'s'", 40, 0, None
    yield "escaped-quote-disabled", 0, r"SELECT E'it\'s'", 40, 16, None


def snapshot(probe, upstream):
    rows = []
    for name, mode, sql, limit, flags, wanted_text in [
        *((name, mode, sql, -1, 0, None) for name, mode, sql in inputs(upstream)),
        *truncation_inputs(upstream)
    ]:
        args = [probe, str(mode), str(limit), str(flags)]
        result = subprocess.run(args, input=sql.encode(), capture_output=True)
        row = {"id": name, "mode": mode, "sql": sql}
        if limit != -1:
            row.update(limit=limit, flags=flags)
        if result.returncode == 0:
            row["protobuf"] = result.stdout.hex()
            if limit != -1:
                text = subprocess.check_output([*args, "text"], input=sql.encode()).decode()
                if wanted_text is not None and wanted_text != text:
                    raise ValueError(f"{name}: legacy expected {wanted_text!r}, PG19 gave {text!r}")
                row["truncated"] = text
        elif result.returncode == 1:
            row["error"] = json.loads(result.stderr)
        else:
            raise ValueError(f"{name}: reference exited {result.returncode}: {result.stderr!r}")
        rows.append(row)
    return rows


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("probe", type=Path)
    parser.add_argument("--write", action="store_true", help="replace the pinned snapshots")
    parser.add_argument("--import", dest="run", type=Path, help="reference import if the probe is outside RUN/summary")
    args = parser.parse_args()
    probe = args.probe.resolve()
    rows = snapshot(probe, (args.run or probe.parent.parent) / "libpg_query")
    if args.write:
        SNAPSHOT.write_text("".join(json.dumps(row, ensure_ascii=False, separators=(",", ":")) + "\n" for row in rows))
    else:
        expected = [json.loads(line) for line in SNAPSHOT.read_text().splitlines()]
        if rows != expected:
            raise ValueError("summary result differs; inspect the regenerated snapshot")
    print(f"{len(rows)} PG19 summary inputs, {sum('error' in row for row in rows)} diagnostics")
