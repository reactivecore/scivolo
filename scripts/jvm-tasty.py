#!/usr/bin/env python3
"""Compare the TASTy files of REF_DIR (scalac's) with those of OUT_DIR (ours) as dotty's
`TastyPrinter` shows them, without the UUID, the name table's size and the sections' offsets, and
with scalac's source path made relative to the working directory. Prints a unified diff per file that differs; fails if any does or is missing.
Both sides are printed from copies in tmp/jvm-tasty whose signed constants of five bytes or more
keep only their last four, as the printer reads them as nats (docs/conformance.md, upstream bug 15);
one of ten bytes is followed by a `UNITconst` in nine. The cut constants are compared in full, in
order, and a file whose cut constants differ fails.
"""

import difflib
import os
import pathlib
import re
import shutil
import subprocess
import sys

PRINTER = ["coursier", "launch", "org.scala-lang:scala3-compiler_3:3.9.0",
           "-M", "dotty.tools.dotc.core.tasty.TastyPrinter", "--", "-color:never"]
RULE = "-" * 80
COPIES = pathlib.Path("tmp/jvm-tasty")
FIRST_NAT, FIRST_AST, FIRST_NAT_AST, FIRST_LENGTH = 60, 90, 110, 128
SIGNED = {67, 68, 70, 71, 72, 73}  # BYTEconst SHORTconst INTconst LONGconst FLOATconst DOUBLEconst
ONE_NAT = {129, 130, 131, 133, 134, 144, 148, 150, 159, 174, 175, 176, 255}  # VALDEF ... HOLE
PARAMTYPE, METHODIC = 172, {169, 170, 180}  # PARAMtype; POLYtype TYPELAMBDAtype METHODtype
NAT_BYTES, LONG_NAT_BYTES, UNITCONST = 4, 9, 2


def nat(b, i):
    """The nat at `i` in `b` and the index after it."""
    n = 0
    while True:
        n, i = (n << 7) | (b[i] & 0x7f), i + 1
        if b[i - 1] & 0x80:
            return n, i


def is_modifier(tag):
    return 6 <= tag <= 44 or tag in (47, 49, 98, 99, 173)  # TRACKED INTO ...qualified ANNOTATION


def tree(b, i, cut):
    """Clears the leading bytes of the long signed constants of the tree at `i`, appending their
    original bytes to `cut`; the index after it."""
    tag, i = b[i], i + 1
    if tag < FIRST_NAT:
        return i
    if tag < FIRST_AST:
        start, (_, i) = i, nat(b, i)
        if tag in SIGNED and i - start > NAT_BYTES:
            cut.append(bytes([tag]) + bytes(b[start:i]))
            b[start:i - NAT_BYTES] = bytes(i - NAT_BYTES - start)
        if tag in SIGNED and i - start > LONG_NAT_BYTES:
            b[start:i] = b[start + 1:i] + bytes([UNITCONST])
        return i
    if tag < FIRST_NAT_AST:
        return tree(b, i, cut)
    if tag < FIRST_LENGTH:
        return tree(b, nat(b, i)[1], cut)
    length, i = nat(b, i)
    end = i + length
    for _ in range(2 if tag == PARAMTYPE else 1 if tag in ONE_NAT else 0):
        i = nat(b, i)[1]
    if tag in METHODIC:
        i = tree(b, i, cut)
        while i < end and not is_modifier(b[i]):
            i = nat(b, tree(b, i, cut))[1]
    while i < end:
        i = tree(b, i, cut)
    return end


def printable(root, name, side, cut):
    """A copy of TASTy file `name` below `root` the printer reads, under COPIES/`side`; appends the
    original bytes of its cut constants to `cut`."""
    b = bytearray((root / name).read_bytes())
    i = 4
    for _ in range(3):
        i = nat(b, i)[1]
    tooling, i = nat(b, i)
    names, i = nat(b, i + tooling + 16)
    table, i = b[i:i + names], i + names
    utf8, j = [], 0
    while j < len(table):
        tag, (size, j) = table[j], nat(table, j + 1)
        utf8.append(bytes(table[j:j + size]) if tag == 1 else None)
        j += size
    while i < len(b):
        (section, i), (size, start) = nat(b, i), nat(b, nat(b, i)[1])
        if utf8[section] == b"ASTs":
            k = start
            while k < start + size:
                k = tree(b, k, cut)
        i = start + size
    copy = COPIES / side / name
    copy.parent.mkdir(parents=True, exist_ok=True)
    copy.write_bytes(b)
    return copy


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
    shutil.rmtree(COPIES, ignore_errors=True)
    cuts = [[] for _ in range(2 * len(both))]
    copies = [printable(ref, n, "scalac", cuts[i]) for i, n in enumerate(both)]
    copies += [printable(out, n, "ours", cuts[i + len(both)]) for i, n in enumerate(both)]
    printed = shown(copies)
    for i, name in enumerate(both):
        left, right = printed[i], printed[i + len(both)]
        diff = list(difflib.unified_diff(left, right, f"scalac/{name}", f"ours/{name}", lineterm=""))
        if cuts[i] != cuts[i + len(both)]:
            hexes = [" ".join(c.hex() for c in cuts[j]) for j in (i, i + len(both))]
            diff += [f"cut constants: scalac [{hexes[0]}], ours [{hexes[1]}]"]
        print("\n".join(diff) if diff else f"{name}: same")
        failed |= bool(diff)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main(*sys.argv[1:])
