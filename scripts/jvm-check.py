#!/usr/bin/env python3
"""Compare the class files of REF_DIR (scalac's) with those of OUT_DIR (ours) through javap.

For each class file: `javap -p -s -c` without constant pool indexes and with members sorted, and
the Signature, InnerClasses, RuntimeVisibleAnnotations and Exceptions lines of `javap -v`, TASTY
by presence. Prints a unified diff per class that differs; fails if any does or is missing.
"""

import difflib
import pathlib
import re
import subprocess
import sys

ATTRIBUTES = re.compile(r"^\s*(Signature|InnerClasses|RuntimeVisibleAnnotations|Exceptions|TASTY):")


def javap(*args):
    return subprocess.run(["javap", *args], capture_output=True, text=True, check=True).stdout


def code(path):
    """The members of `javap -p -s -c`, each a block of lines, sorted; the class header first."""
    lines = [re.sub(r"#\d+(, *\d+)?\s*", "", l) for l in javap("-p", "-s", "-c", str(path)).splitlines()]
    lines = [l for l in lines if not l.startswith("Compiled from")]
    header, body = lines[0], "\n".join(lines[1:-1])
    members = sorted(m.strip("\n") for m in body.split("\n\n") if m.strip())
    return [header] + [l for m in members for l in m.splitlines() + [""]]


def attributes(path):
    out = []
    for l in javap("-v", str(path)).splitlines():
        if m := ATTRIBUTES.match(l):
            out.append("TASTY:" if m.group(1) == "TASTY" else re.sub(r"#\d+\s*", "", l.strip()))
    return out


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
