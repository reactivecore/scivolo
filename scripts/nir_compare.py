#!/usr/bin/env python3
"""Scala Native's NIR compared with ours, as `scivolo-nir-dump --debug --sorted` dumps them. The
var slots of a method with more than four mutable locals are in the order of dotty's symbol ids
in Scala Native's (nscplugin's `CollectMethodInfo.mutableVars`, a `HashSet` from five elements
on); they are matched with ours by name and type.

Usage: scripts/nir_compare.py THEIRS OURS: prints dump THEIRS with the var slots of such methods
renumbered and ordered as in dump OURS.
"""
import collections
import difflib
import re
import subprocess
import sys
from pathlib import Path

NIR_DUMP = Path(__file__).resolve().parent.parent / "target/release/scivolo-nir-dump"
SET_SIZE = 4
VAR = re.compile(r"^  (?:\d+: )?%(\d+) <([^>]*)> = var\[(.*)\]$")
ID = re.compile(r"%(\d+)\b")
NAME = re.compile(r"\((\d+), (\"(?:[^\"\\]|\\.)*\")\)")


def methods(lines):
    """The index of each method definition's header line, by header."""
    return {l: i for i, l in enumerate(lines) if l.endswith("{") and re.match(r"^(\w+ )*def @", l)}


def slots(lines, i):
    """(id, name, type) of the var slots after the entry label of the method at line `i`."""
    found = []
    for l in lines[i + 2:]:
        if not (m := VAR.match(l)):
            break
        found.append((int(m[1]), m[2], m[3]))
    return found


def renumber(lines, i, ids):
    """The method at line `i` with var ids renamed by `ids` and its var slots in id order."""
    end = next((j for j in range(i, len(lines)) if lines[j] == ""), len(lines))
    block = lines[i:end]
    sub = lambda m: f"%{ids.get(int(m[1]), int(m[1]))}"
    for k, l in enumerate(block):
        if l.startswith("names ["):
            names = sorted((ids.get(int(n), int(n)), s) for n, s in NAME.findall(l))
            block[k] = "names [" + ", ".join(f"({n}, {s})" for n, s in names) + "]"
        elif not l.startswith(("@ ", "scope ")):
            block[k] = ID.sub(sub, l)
    close = block.index("}")
    n = len(slots(block, 0))
    order = sorted(range(2, 2 + n), key=lambda k: int(VAR.match(block[k])[1]))
    at = [k for k, l in enumerate(block) if l.startswith("@ ")]
    block[2:2 + n] = [block[k] for k in order]
    if len(at) == close and at == list(range(close + 1, 2 * close + 1)):
        block[close + 3:close + 3 + n] = [block[close + 1 + k] for k in order]
    return lines[:i] + block + lines[end:]


def align(theirs, ours):
    """`theirs` with the var slots of each method with more than SET_SIZE of them, whose slots
    in `ours` have the same ids, names and types, renumbered as in `ours`; and whether that
    changed it."""
    mine, changed = methods(ours), False
    for header, i in methods(theirs).items():
        a = slots(theirs, i)
        if len(a) <= SET_SIZE or header not in mine:
            continue
        b = slots(ours, mine[header])
        if (sorted(s[1:] for s in a) != sorted(s[1:] for s in b) or [s[1:] for s in a] == [s[1:] for s in b]
                or sorted(s[0] for s in a) != sorted(s[0] for s in b)):
            continue
        free = collections.defaultdict(collections.deque)
        for n, *key in b:
            free[tuple(key)].append(n)
        theirs = renumber(theirs, i, {n: free[tuple(key)].popleft() for n, *key in a})
        changed = True
    return theirs, changed


def dump(path):
    r = subprocess.run([str(NIR_DUMP), "--debug", "--sorted", str(path)], capture_output=True, text=True,
                       errors="replace")
    if r.returncode != 0:
        return [f"nir-dump failed on {path}: {r.stderr.strip()}"]
    return [l for l in r.stdout.splitlines() if not l.startswith(("// ", "# "))]


def compare(ours, theirs, diff_file):
    """`same`, `differ N` or `var-order N` (changed lines once the var slots are aligned, the diff
    written to `diff_file`) or `missing`."""
    if not theirs.exists():
        return "missing", 0
    mine = dump(ours)
    aligned, reordered = align(dump(theirs), mine)
    diff = list(difflib.unified_diff(aligned, mine, "oracle", "ours", lineterm="", n=0))
    changed = sum(1 for l in diff if l[:1] in "+-" and not l.startswith(("+++", "---")))
    if changed:
        diff_file.parent.mkdir(parents=True, exist_ok=True)
        diff_file.write_text("\n".join(diff) + "\n")
    return "var-order" if reordered else "differ" if changed else "same", changed


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    theirs, ours = (open(p).read().splitlines() for p in sys.argv[1:])
    print("\n".join(align(theirs, ours)[0]))
