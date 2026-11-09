#!/usr/bin/env python3
"""Compare the class files of REF_DIR (scalac's) with those of OUT_DIR (ours) through javap.

For each class file: `javap -p -s -c` without constant pool indexes and with members sorted, and
from `javap -v` each member's flags, attributes and line numbers (sorted by member), the class's
attributes, unknown ones by name, and the flags of its `InnerClasses` entries. Prints a unified diff per class that differs; fails if any does or is missing.
"""

import difflib
import pathlib
import re
import subprocess
import sys

MEMBER_ATTRIBUTES = {
    "flags", "Signature", "Exceptions", "RuntimeVisibleAnnotations",
    "RuntimeVisibleParameterAnnotations", "MethodParameters", "Deprecated", "AnnotationDefault",
}
CLASS_ATTRIBUTES = {
    "flags", "Signature", "InnerClasses", "EnclosingMethod", "RuntimeVisibleAnnotations", "Deprecated",
    "SourceFile",
}
LINES = {"LineNumberTable"}
UNCOMPARED = {"Scala3InlineInfo"}


UNKNOWN = re.compile(r"^\s*\w+: length = \w+ \(unknown attribute\)")


def javap(*args):
    return subprocess.run(["javap", *args], capture_output=True, text=True, check=True).stdout


def clean(line):
    return re.sub(r"\s+", " ", re.sub(r"#\d+(, *\d+)?\s*", "", line.strip()))


def code(path):
    """The members of `javap -p -s -c`, each a block of lines, sorted; the class header first."""
    lines = [re.sub(r"#\d+(, *\d+)?\s*", "", l) for l in javap("-p", "-s", "-c", str(path)).splitlines()]
    lines = [l for l in lines if not l.startswith("Compiled from")]
    header, body = lines[0], "\n".join(lines[1:-1])
    members = sorted(m.strip("\n") for m in body.split("\n\n") if m.strip())
    return [header] + [l for m in members for l in m.splitlines() + [""]]


def sections(lines, indent, keep):
    """The sections in `lines` whose name in `keep` starts a line at `indent`, with their deeper
    lines, sorted by name."""
    out, on = [], False
    for l in lines:
        depth = len(l) - len(l.lstrip())
        if UNKNOWN.match(l):
            name = l.strip().split(":")[0]
            at_class = indent == 0 and depth == 2 and name not in UNCOMPARED
            if (depth == indent or at_class) and keep is not LINES:
                out.append([name + ": (unknown attribute)"])
            on = False
        elif depth == indent and l.strip():
            name = l.strip().split(":")[0]
            on = name in keep
            if on:
                out.append([clean(l)])
        elif depth > indent and on:
            out[-1].append("  " + clean(l))
    return [l for s in sorted(out) for l in s]


def attributes(path):
    text = javap("-v", "-p", str(path)).splitlines()
    start, end = text.index("{"), len(text) - 1 - text[::-1].index("}")
    members = []
    for block in "\n".join(text[start + 1:end]).split("\n\n"):
        lines = [l for l in block.splitlines() if l.strip()]
        if lines:
            lines_table = sections(lines[1:], 6, LINES)
            members.append([lines[0].strip()] + sections(lines[1:], 4, MEMBER_ATTRIBUTES) + lines_table)
    head = [l for l in text[:start] if l.strip().startswith("flags:")]
    out = [clean(l) for l in head] + sections(text[end + 1:], 0, CLASS_ATTRIBUTES)
    return out + inner_class_flags(path) + [l for m in sorted(members) for l in m]


def class_attributes(b):
    """The constant pool of class file bytes `b` (index to UTF-8 text or referenced index) and its
    class attributes, each (name, offset of its body, length)."""
    u2 = lambda i: int.from_bytes(b[i:i + 2], "big")
    pool, i, n = {}, 10, 1
    while n < u2(8):
        tag = b[i]
        size = {1: 3 + u2(i + 1), 5: 9, 6: 9, 15: 4}.get(tag, 3 if tag in (7, 8, 16, 19, 20) else 5)
        pool[n] = b[i + 3:i + size].decode("utf-8", "replace") if tag == 1 else u2(i + 1)
        n, i = n + (2 if tag in (5, 6) else 1), i + size
    i += 6
    i += 2 + 2 * u2(i)
    for _ in range(2):
        count, i = u2(i), i + 2
        for _ in range(count):
            attrs, i = u2(i + 6), i + 8
            for _ in range(attrs):
                i += 6 + int.from_bytes(b[i + 2:i + 6], "big")
    attributes, count, i = [], u2(i), i + 2
    for _ in range(count):
        length = int.from_bytes(b[i + 2:i + 6], "big")
        attributes.append((pool[u2(i)], i + 6, length))
        i += 6 + length
    return pool, attributes


def inner_class_flags(path):
    """The `InnerClasses` entries with their flags in hex, which `javap` shows only in part."""
    b = path.read_bytes()
    u2 = lambda i: int.from_bytes(b[i:i + 2], "big")
    pool, attributes = class_attributes(b)
    name = lambda k, cls: (pool[pool[k]] if cls else pool[k]) if k else "-"
    out = []
    for attribute, i, _ in attributes:
        if attribute == "InnerClasses":
            for e in range(u2(i)):
                j = i + 2 + 8 * e
                inner, outer, simple = name(u2(j), True), name(u2(j + 2), True), name(u2(j + 4), False)
                out.append(f"  inner class {inner} {outer} {simple} 0x{u2(j + 6):04x}")
    return sorted(out)


def main(ref, out):
    ref, out = pathlib.Path(ref), pathlib.Path(out)
    failed = False
    names = sorted({p.relative_to(d) for d in (ref, out) for p in d.rglob("*.class")})
    for name in names:
        a, b = ref / name, out / name
        if not a.exists() or not b.exists():
            print(f"{name}: only in {'ours' if b.exists() else 'scalac'}")
            failed = True
            continue
        left, right = code(a) + attributes(a), code(b) + attributes(b)
        diff = list(difflib.unified_diff(left, right, f"scalac/{name}", f"ours/{name}", lineterm=""))
        print("\n".join(diff) if diff else f"{name}: same")
        failed |= bool(diff)
    for name in sorted({p.relative_to(d) for d in (ref, out) for p in d.rglob("*.tasty")}):
        if not (ref / name).exists() or not (out / name).exists():
            print(f"{name}: only in {'ours' if (out / name).exists() else 'scalac'}")
            failed = True
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main(*sys.argv[1:])
