#!/usr/bin/env python3
"""The `pub fn`s of 30 or more lines whose doc comment has at most one line.

Usage: scripts/doc_gaps.py [--per-crate] [CRATE...]

Scans crates/*/src (or the named crates), tests and `#[cfg(test)]` items left out. Prints
`PATH:LINE NAME (N lines)` for each and the total, or one total per crate with --per-crate.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MIN_LINES = 30
PUB_FN = re.compile(r"^\s*pub\s+(?:const\s+)?(?:unsafe\s+)?(?:extern\s+\"C\"\s+)?fn\s+(\w+)")


def code_only(text):
    """Replaces comments, strings and char literals by spaces, keeping the line breaks."""
    out, i, n = [], 0, len(text)
    while i < n:
        c = text[i]
        if text.startswith("//", i):
            j = text.find("\n", i)
            i = n if j < 0 else j
        elif text.startswith("/*", i):
            j = text.find("*/", i + 2)
            j = n if j < 0 else j + 2
            out.append("\n" * text.count("\n", i, j))
            i = j
        elif m := re.compile(r'b?r(#*)"').match(text, i):
            end = '"' + m.group(1)
            j = text.find(end, m.end())
            j = n if j < 0 else j + len(end)
            out.append("\n" * text.count("\n", i, j))
            i = j
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            out.append("\n" * text.count("\n", i, j))
            i = j + 1
        elif m := re.compile(r"'(\\.[^']*|[^\\'])'").match(text, i):
            i = m.end()
        else:
            out.append(c)
            i += 1
    return "".join(out).split("\n")


def body_lines(code, start):
    """Counts the lines from `start` to the line that closes the function's body."""
    depth, opened = 0, False
    for k in range(start, len(code)):
        for c in code[k]:
            if c == "{":
                depth, opened = depth + 1, True
            elif c == "}":
                depth -= 1
            elif c == ";" and not opened and depth == 0:
                return 0
        if opened and depth == 0:
            return k - start + 1
    return 0


def doc_lines(lines, start):
    """Counts the `///` lines above line `start`, attributes skipped."""
    k, docs = start - 1, 0
    while k >= 0:
        s = lines[k].strip()
        if s.startswith("///"):
            docs += 1
        elif not s.startswith("#["):
            break
        k -= 1
    return docs


def gaps(path):
    text = path.read_text()
    lines, code = text.split("\n"), code_only(text)
    skip_to = -1
    for k, line in enumerate(lines):
        if k <= skip_to:
            continue
        if line.strip() == "#[cfg(test)]":
            skip_to = k + body_lines(code, k + 1)
            continue
        m = PUB_FN.match(line)
        if m and doc_lines(lines, k) <= 1 and (n := body_lines(code, k)) >= MIN_LINES:
            yield k + 1, m.group(1), n


def main(args):
    per_crate = "--per-crate" in args
    names = [a for a in args if a != "--per-crate"]
    if any(a.startswith("-") for a in names):
        sys.exit(__doc__)
    crates = [ROOT / "crates" / c for c in names] or sorted((ROOT / "crates").iterdir())
    total = 0
    for crate in crates:
        found = [
            (f, *g)
            for f in sorted((crate / "src").rglob("*.rs"))
            if "tests" not in f.relative_to(crate).parts and f.name != "tests.rs"
            for g in gaps(f)
        ]
        total += len(found)
        if per_crate:
            print(f"{len(found):5} {crate.name}")
        else:
            for f, line, name, n in found:
                print(f"{f.relative_to(ROOT)}:{line} {name} ({n} lines)")
    print(f"{total:5} total" if per_crate else f"total: {total}")


if __name__ == "__main__":
    main(sys.argv[1:])
