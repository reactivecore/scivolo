#!/usr/bin/env python3
"""Scala Native's NIR compared with ours, as `scivolo-nir-dump --debug --sorted` dumps them. The
var slots of a method with more than four mutable locals are in the hash order of dotty's symbol
ids in Scala Native's (nscplugin's `CollectMethodInfo.mutableVars`, a `HashSet` from five
elements on); they are matched with ours by name and type.

Usage: scripts/nir_compare.py THEIRS OURS: prints dump THEIRS with the var slots of such methods
renumbered and ordered as in dump OURS.
"""
import base64
import collections
import difflib
import hashlib
import itertools
import re
import subprocess
import sys
from pathlib import Path

NIR_DUMP = Path(__file__).resolve().parent.parent / "target/release/scivolo-nir-dump"
SET_SIZE = 4
CANDIDATES = 120
VAR = re.compile(r"^  (?:\d+: )?%(\d+) <([^>]*)> = var\[(.*)\]$")
ID = re.compile(r"%(\d+)\b")
NAME = re.compile(r"\((\d+), (\"(?:[^\"\\]|\\.)*\")\)")
QUOTE = re.compile(r'"(XKGr[A-Za-z0-9+/=]*)"')
TASTY_MAGIC = bytes([0x5C, 0xA1, 0xAB, 0x1F])
POSITIONS_SOURCE = 4


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
    block = method(lines, i)
    end = i + len(block)
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
    in `ours` have the same ids, names and types, renumbered as in `ours` (between slots of the
    same name and type, as makes the method as in `ours` if any such mapping does); and whether
    that changed it."""
    mine, changed = methods(ours), False
    for header, i in methods(theirs).items():
        a = slots(theirs, i)
        if len(a) <= SET_SIZE or header not in mine:
            continue
        b = slots(ours, mine[header])
        if (sorted(s[1:] for s in a) != sorted(s[1:] for s in b) or [s[1:] for s in a] == [s[1:] for s in b]
                or sorted(s[0] for s in a) != sorted(s[0] for s in b)):
            continue
        mapped = (renumber(theirs, i, ids) for ids in itertools.islice(mappings(a, b), CANDIDATES))
        first, target = next(mapped), method(ours, mine[header])
        theirs = next((m for m in itertools.chain([first], mapped) if method(m, i) == target), first)
        changed = True
    return theirs, changed


def mappings(a, b):
    """Each mapping of the ids of slots `a` to those of slots `b` of the same name and type."""
    groups = collections.defaultdict(lambda: ([], []))
    for side, found in enumerate((a, b)):
        for n, *key in found:
            groups[tuple(key)][side].append(n)
    keys = list(groups)
    for perms in itertools.product(*(itertools.permutations(groups[k][1]) for k in keys)):
        yield {t: o for k, p in zip(keys, perms) for t, o in zip(groups[k][0], p)}


def method(lines, i):
    """The lines of the method at line `i`."""
    return lines[i:next((j for j in range(i, len(lines)) if lines[j] == ""), len(lines))]


def dump(path):
    r = subprocess.run([str(NIR_DUMP), "--debug", "--sorted", str(path)], capture_output=True, text=True,
                       errors="replace")
    if r.returncode != 0:
        return [f"nir-dump failed on {path}: {r.stderr.strip()}"]
    return [l for l in r.stdout.splitlines() if not l.startswith(("// ", "# "))]


def nat(data, i):
    """The TASTy natural number at `data[i]` and the index after it."""
    x = 0
    while True:
        b = data[i]
        i += 1
        x = x << 7 | b & 0x7F
        if b & 0x80:
            return x, i


def long_int(data, i):
    """The TASTy signed number at `data[i]` and the index after it."""
    b = data[i]
    x = (b & 0x7F) - (0x80 if b & 0x40 else 0)
    while not b & 0x80:
        i += 1
        b = data[i]
        x = x << 7 | b & 0x7F
    return x, i + 1


def source_names(positions):
    """The name indices the Positions section `positions` refers to for its sources."""
    lines, i = nat(positions, 0)
    for _ in range(lines):
        _, i = nat(positions, i)
    found = set()
    while i < len(positions):
        header, i = long_int(positions, i)
        if header == POSITIONS_SOURCE:
            ref, i = nat(positions, i)
            found.add(ref)
            continue
        for bit in (4, 2, 1):
            if header & bit:
                _, i = long_int(positions, i)
    return found


def quote_key(text):
    """A key of the TASTy quote Base64 `text` without its header's UUID and its positions: its
    names but those only positions use, and its ASTs; None if `text` is no TASTy."""
    try:
        data = base64.b64decode(text, validate=True)
        if data[:4] != TASTY_MAGIC:
            return None
        i = 4
        for _ in range(3):
            _, i = nat(data, i)
        n, i = nat(data, i)
        i += n + 16
        size, i = nat(data, i)
        end, names = i + size, []
        while i < end:
            n, start = nat(data, i + 1)
            names.append((data[i:start + n], data[start:start + n]))
            i = start + n
        sections = {}
        while i < len(data):
            ref, i = nat(data, i)
            n, i = nat(data, i)
            sections[names[ref][1].decode(errors="replace")] = (ref, data[i:i + n])
            i += n
        dropped = {r for k, (r, _) in sections.items() if k != "ASTs"}
        if "Positions" in sections:
            dropped |= source_names(sections["Positions"][1])
        kept = b"".join(entry for k, (entry, _) in enumerate(names) if k not in dropped)
        return hashlib.sha1(kept + b"|" + sections.get("ASTs", (0, b""))[1]).hexdigest()[:16]
    except (IndexError, ValueError, KeyError):
        return None


def without_quote_positions(lines):
    """`lines` with each string literal of a TASTy quote replaced by its `quote_key`."""
    sub = lambda m: f'"<quote {k}>"' if (k := quote_key(m[1])) else m[0]
    return [QUOTE.sub(sub, l) for l in lines]


def compare(ours, theirs, diff_file):
    """(`same`, `var-order` (same once the var slots are aligned), `quote-pos` (same but for the
    positions in TASTy quotes), `differ` or `missing`; the lines changed once they are aligned,
    the diff written to `diff_file`; whether they were aligned)."""
    if not theirs.exists():
        return "missing", 0, False
    mine = dump(ours)
    aligned, reordered = align(dump(theirs), mine)
    if aligned != mine and (keyed := without_quote_positions(aligned)) != aligned:
        if keyed == without_quote_positions(mine):
            return "quote-pos", 0, reordered
        aligned, mine = keyed, without_quote_positions(mine)
    diff = list(difflib.unified_diff(aligned, mine, "oracle", "ours", lineterm="", n=0))
    changed = sum(1 for l in diff if l[:1] in "+-" and not l.startswith(("+++", "---")))
    if changed:
        diff_file.parent.mkdir(parents=True, exist_ok=True)
        diff_file.write_text("\n".join(diff) + "\n")
    return "differ" if changed else "var-order" if reordered else "same", changed, reordered


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    theirs, ours = (open(p).read().splitlines() for p in sys.argv[1:])
    print("\n".join(align(theirs, ours)[0]))
