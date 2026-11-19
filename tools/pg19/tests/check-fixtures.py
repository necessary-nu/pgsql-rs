"""Check stored reference outcomes without compiling the C probes."""
import json
from pathlib import Path


def check_tokens(sql, tokens):
    data = sql.encode()
    previous_end = 0
    for token in tokens:
        start, end = token["start"], token["end"]
        assert previous_end <= start < end <= len(data), token
        spelling = data[start:end].decode()
        assert token["name"] and token["id"] > 0, token
        if token["id"] < 256:
            assert spelling == token["name"] == chr(token["id"]), token
        previous_end = end


def check_fixture(path, flavor):
    sql = path.read_text(encoding="utf-8")
    snapshot = path.with_suffix(f".{flavor}.json")
    result = json.loads(snapshot.read_bytes())
    assert isinstance(result["tokens"], list)
    check_tokens(sql, result["tokens"])
    parse_error = result["parse_error"]
    assert (parse_error is None) == isinstance(result["tree"], str)
    for error in (parse_error, result["scan_error"]):
        if error is not None:
            assert len(error["sqlstate"]) == 5 and error["message"]
            assert 0 <= error["position"] <= len(sql) + 1
    if path.stem == "set-role":
        if flavor == "stock":
            assert parse_error["sqlstate"] == "42601" and parse_error["position"] == 10
        else:
            assert "{PARAMREF :number 1 :location 9}" in result["tree"]
            assert ":is_local false :location 9}" in result["tree"]
    if path.stem == "utf8-error":
        assert parse_error["position"] == sql.index(")") + 1
        assert result["scan_error"] is None

    if path.stem in ("assign1", "assign2", "assign3"):
        assert "{PLASSIGNSTMT :name v " in result["tree"]
        assert f" :nnames {path.stem[-1]} " in result["tree"]
    if path.stem == "expr":
        assert "{NULLTEST :arg {COLUMNREF" in result["tree"]
    if path.stem == "type-name":
        assert result["tree"].startswith('({TYPENAME :names ("pg_catalog" "varchar")')
        assert ":typmods ({A_CONST :val 32 " in result["tree"]
    if path.stem == "quote-off":
        assert result["scan_error"] == parse_error
        assert parse_error["sqlstate"] == "22P06"
        assert parse_error["position"] == sql.index("E'") + 1
    if path.stem in ("quote-on", "quote-safe"):
        assert parse_error is None and result["scan_error"] is None
        assert "{A_CONST :val \"it's\"" in result["tree"]


if __name__ == "__main__":
    fixtures = Path(__file__).resolve().parents[1] / "fixtures"
    inputs = sorted(fixtures.glob("*.sql"))
    assert inputs, "missing PG19 SQL corpus"
    expected = set()
    for path in inputs:
        for flavor in ("stock", "patched"):
            expected.add(path.with_suffix(f".{flavor}.json"))
            try:
                check_fixture(path, flavor)
            except (AssertionError, ValueError, KeyError, TypeError) as exc:
                raise ValueError(f"invalid {flavor} snapshot for {path.name}") from exc
    assert set(fixtures.glob("*.json")) == expected, "orphaned PG19 snapshot"
