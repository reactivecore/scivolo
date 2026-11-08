#!/usr/bin/env python3
"""Compare the TASTy files of REF_DIR (scalac's) with those of OUT_DIR (ours) as dotty's
`TastyPrinter` shows them, without the UUID, the name table's size and the sections' offsets, and
with scalac's source path made relative to the working directory. Prints a unified diff per file that differs; fails if any does or is missing.
"""

import difflib
import os
import pathlib
import re
import subprocess
import sys

PRINTER = ["coursier", "launch", "org.scala-lang:scala3-compiler_3:3.9.0",
           "-M", "dotty.tools.dotc.core.tasty.TastyPrinter", "--", "-color:never"]
RULE = "-" * 80


def shown(paths):
    """The printout of each file in `paths`, by path."""
    out = subprocess.run(PRINTER + [str(p) for p in paths], capture_output=True, text=True, check=True).stdout
    blocks, cwd = {}, os.getcwd() + "/"
    parts = out.split(RULE + "\n")
    for name, body in zip(parts[1::2], parts[2::2]):
        body = re.sub(r"^\s*UUID: .*\n", "", body.replace(cwd, ""), flags=re.M)
        body = re.sub(r"Names \(\d+ bytes|, starting from \d+", "", body)
        blocks[name.strip()] = body.splitlines()
    return [blocks.get(str(p), []) for p in paths]


def main(ref, out):
    ref, out = pathlib.Path(ref), pathlib.Path(out)
    names = sorted({p.relative_to(d) for d in (ref, out) for p in d.rglob("*.tasty")})
    both = [n for n in names if (ref / n).exists() and (out / n).exists()]
    failed = len(both) != len(names)
    for n in names:
        if n not in both:
            print(f"{n}: only in {'ours' if (out / n).exists() else 'scalac'}")
    if not both:
        sys.exit(1 if failed else 0)
    printed = shown([ref / n for n in both] + [out / n for n in both])
    for i, name in enumerate(both):
        left, right = printed[i], printed[i + len(both)]
        diff = list(difflib.unified_diff(left, right, f"scalac/{name}", f"ours/{name}", lineterm=""))
        print("\n".join(diff) if diff else f"{name}: same")
        failed |= bool(diff)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main(*sys.argv[1:])
