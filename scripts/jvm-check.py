#!/usr/bin/env python3
"""Compare the class files of REF_DIR (scalac's) with those of OUT_DIR (ours) through javap.

For each class file: `javap -p -s -c` without constant pool indexes and with members sorted, and
from `javap -v` each member's flags and attributes (sorted by member) and the class's attributes,
TASTY by presence. Prints a unified diff per class that differs; fails if any does or is missing.
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
    "TASTY",
}


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
            if "TASTY" in keep and l.strip().startswith("TASTY:"):
                out.append(["TASTY:"])
            on = False
        elif depth == indent and l.strip():
            name = l.strip().split(":")[0]
            on = name in keep
            if on:
                out.append(["TASTY:" if name == "TASTY" else clean(l)])
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
            members.append([lines[0].strip()] + sections(lines[1:], 4, MEMBER_ATTRIBUTES))
    head = [l for l in text[:start] if l.strip().startswith("flags:")]
    out = [clean(l) for l in head] + sections(text[end + 1:], 0, CLASS_ATTRIBUTES)
    return out + [l for m in sorted(members) for l in m]


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
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main(*sys.argv[1:])
