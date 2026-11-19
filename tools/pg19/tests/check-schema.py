"""Cross-check the PG19 descriptor with patched C reference snapshots."""
import json
from pathlib import Path

from google.protobuf.descriptor_pb2 import FileDescriptorSet
from google.protobuf.descriptor_pool import DescriptorPool


def check_schema():
    tools = Path(__file__).resolve().parents[1]
    pinned = tools.parents[1] / "protobuf" / "pg19" / "pg_query.pb"
    pool = DescriptorPool()
    for file in FileDescriptorSet.FromString(pinned.read_bytes()).file:
        pool.Add(file)
    token_enum = pool.FindEnumTypeByName("pg_query.Token")
    fixtures = tools / "fixtures"
    for path in fixtures.glob("*.patched.json"):
        for token in json.loads(path.read_bytes())["tokens"]:
            symbol = token["name"] if token["id"] >= 256 else f"ASCII_{token['id']}"
            assert token_enum.values_by_name[symbol].number == token["id"], (path, token)

    node = pool.FindMessageTypeByName("pg_query.Node")
    for case, arm, field_tags in [
        ("wait", "wait_stmt", {"lsn_literal": 1, "options": 2, "lsn_location": 3}),
        ("repack", "repack_stmt", {"command": 1, "relation": 2, "indexname": 3,
                                   "usingindex": 4, "params": 5}),
    ]:
        tree = json.loads((fixtures / f"{case}.patched.json").read_bytes())["tree"]
        field = node.fields_by_name[arm]
        assert field.containing_oneof.name == "node"
        assert "{" + field.message_type.name.upper() + " " in tree
        for name, tag in field_tags.items():
            assert field.message_type.fields_by_name[name].number == tag
            assert f" :{name} " in tree

    clause = pool.FindMessageTypeByName("pg_query.OnConflictClause")
    action = clause.fields_by_name["action"].enum_type
    tree = json.loads((fixtures / "on-conflict.patched.json").read_bytes())["tree"]
    assert "{ONCONFLICTCLAUSE :action 3 " in tree
    assert action.values_by_name["ONCONFLICT_SELECT"].number == 4


if __name__ == "__main__":
    check_schema()
