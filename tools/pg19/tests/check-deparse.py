"""Verify the native deparser's input fixture and reparse its SQL using PostgreSQL 19."""
import argparse
import json
from pathlib import Path
import subprocess

from google.protobuf import descriptor_pb2, descriptor_pool, json_format, message_factory


ROOT = Path(__file__).resolve().parents[3]
FIXTURES = ROOT / "crates/pg19-operations/tests"


def without_locations(obj):
    if isinstance(obj, list):
        return [without_locations(item) for item in obj]
    if isinstance(obj, dict):
        return {key: without_locations(value) for key, value in obj.items()
                if key not in ("location", "stmt_len") and not key.endswith("_location")}
    return obj


def check(probe, write):
    def parse(filename):
        sql = (FIXTURES / filename).read_text()
        return json.loads(subprocess.check_output([probe, sql]))

    original = parse("deparse.sql")
    if without_locations(original) != without_locations(parse("deparse.expected.sql")):
        raise ValueError("deparsed SQL reparses to a different PG19 raw tree")
    pool = descriptor_pool.DescriptorPool()
    for file in descriptor_pb2.FileDescriptorSet.FromString(
            (ROOT / "protobuf/pg19/pg_query.pb").read_bytes()).file:
        pool.Add(file)
    desc = pool.FindMessageTypeByName("pg_query.ParseResult")
    if hasattr(message_factory, "GetMessageClass"):
        cls = message_factory.GetMessageClass(desc)
    else:
        cls = message_factory.MessageFactory(pool).GetPrototype(desc)
    wire = json_format.ParseDict(original, cls(), descriptor_pool=pool).SerializeToString(deterministic=True)
    path = FIXTURES / "deparse.pb"
    if write:
        path.write_bytes(wire)
    elif path.read_bytes() != wire:
        raise ValueError("deparse.pb differs from the PG19 C parser")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("json_probe", type=Path)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    check(args.json_probe, args.write)
