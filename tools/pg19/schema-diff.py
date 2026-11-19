"""Describe API changes between two protoc FileDescriptorSets."""
import sys
from itertools import groupby
from pathlib import Path

from google.protobuf.descriptor_pb2 import FieldDescriptorProto, FileDescriptorSet


def definitions(path):
    files = FileDescriptorSet.FromString(Path(path).read_bytes()).file
    messages, enums = {}, {}

    def collect(scope, prefix):
        for enum in scope.enum_type:
            enums[prefix + enum.name] = {v.name: v.number for v in enum.value}
        nested = scope.nested_type if hasattr(scope, "nested_type") else scope.message_type
        for message in nested:
            name = prefix + message.name
            messages[name] = message
            collect(message, name + ".")

    for file in files:
        collect(file, file.package + ".")
    return messages, enums


def fields(message):
    fields = {}
    for field in message.field:
        typ = field.type_name or FieldDescriptorProto.Type.Name(field.type)[5:].lower()
        if field.label == FieldDescriptorProto.LABEL_REPEATED:
            typ = "repeated " + typ
        if field.HasField("oneof_index"):
            typ += f" (oneof {message.oneof_decl[field.oneof_index].name})"
        fields[field.name] = f"{field.number} {typ} json={field.json_name}"
    return fields


def changes(old, new):
    for name in sorted(old.keys() | new.keys()):
        if old.get(name) != new.get(name):
            yield name, old.get(name), new.get(name)


def compare(before, after):
    old_messages, old_enums = definitions(before)
    new_messages, new_enums = definitions(after)
    for name, old, new in changes(old_messages, new_messages):
        if old is None or new is None:
            yield f"message {name}: {'added' if old is None else 'removed'}"
            continue
        for field, a, b in changes(fields(old), fields(new)):
            yield f"field {name}.{field}: {a or '-'} -> {b or '-'}"

    for name, old, new in changes(old_enums, new_enums):
        if old is None or new is None:
            yield f"enum {name}: {'added' if old is None else 'removed'}"
            continue
        for key in sorted(old.keys() - new.keys()):
            yield f"enum {name}.{key}: {old[key]} -> -"
        for key in sorted(new.keys() - old.keys()):
            yield f"enum {name}.{key}: - -> {new[key]}"
        # Keep shifted keyword/token ranges readable. A hole ends a range.
        moved = sorted((old[k], new[k], k) for k in old.keys() & new.keys() if old[k] != new[k])
        for _, group in groupby(enumerate(moved), key=lambda x: (x[1][0] - x[0], x[1][1] - x[0])):
            run = [item for _, item in group]
            a, b, first = run[0]
            c, d, last = run[-1]
            if len(run) == 1:
                yield f"enum {name}.{first}: {a} -> {b}"
            else:
                yield f"enum {name}.{first}..{last}: {a}..{c} -> {b}..{d}"


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit("usage: schema-diff.py OLD.pb NEW.pb")
    print("\n".join(compare(*sys.argv[1:])))
