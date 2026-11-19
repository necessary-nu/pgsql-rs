"""Produce NUL-delimited benchmark input from the captured PG19 regressions."""
import gzip
import hashlib
import json
import sys
from pathlib import Path

out = Path(sys.argv[2])
corpus = Path(sys.argv[1])
queries = []
with (corpus / "cases.jsonl").open() as sql, gzip.open(corpus / "outcomes.jsonl.gz", "rt") as outcomes:
    for line, observed in zip(sql, outcomes, strict=True):
        case, answer = json.loads(line), json.loads(observed)
        if case["id"] != answer["id"]:
            raise ValueError(f"case {case['id']} paired with outcome {answer['id']}")
        if case["mode"] == "default" and answer["patched"]["parse_error"] is None:
            q = case["sql"].encode()
            if b"\0" in q:
                raise ValueError(f"case {case['id']} contains the benchmark delimiter")
            queries.append(q)
if not queries:
    raise ValueError("empty successful SQL corpus")
join = (b"SELECT o.customer_id, count(*), sum(i.quantity * i.price) FROM orders AS o "
        b"JOIN order_items AS i ON i.order_id = o.id WHERE o.created_at >= $1 "
        b"GROUP BY o.customer_id HAVING count(*) > 1 ORDER BY 3 DESC LIMIT 20")
for name, values in {"corpus": queries, "simple": [b"SELECT 1"], "join": [join]}.items():
    data = b"\0".join(values) + b"\0"
    (out / f"{name}.sql0").write_bytes(data)
    print(name, len(values), hashlib.sha256(data).hexdigest())
