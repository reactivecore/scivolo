#!/usr/bin/env python3
"""The Rust typer against dotty 3.9.0's on tests/programs, tests/pos and tests/neg
(docs/research/09-typer.md). A neg file passes when the typer does not accept it: it reports an
error or falls back.

Usage: scripts/typer-tests.py oracle|report|stress [FILTER] | diff PATH | checks DIR | stats
"""
import difflib
import re
import subprocess
import sys
from pathlib import Path

from typer_corpus import DUMP, WORK, load_expected, relative, selected, split, write_list
from typer_oracle import checks, oracle, stats
from typer_report import report
from typer_stress import stress


def diff(path):
    rel = relative(str(Path(path).resolve()))
    opts = next((o for r, o, _ in selected(rel) if r == rel), "")
    lst = write_list("one.tsv", [(rel, opts)])
    out = subprocess.run([str(DUMP), "--list", str(lst)], capture_output=True, text=True, encoding="utf-8")
    actual = split(out.stdout).get(rel, "")
    expected = load_expected().get(rel, "")
    lines = lambda s: [l + "\n" for l in re.sub(r"(\((?=[A-Z]))", r"\n\1", s).splitlines() if not l.startswith("== stats")]
    sys.stdout.writelines(difflib.unified_diff(lines(expected), lines(actual), "dotty", "rust"))


if __name__ == "__main__":
    commands = {"oracle": oracle, "report": report, "stress": stress, "diff": diff, "checks": checks, "stats": stats}
    if len(sys.argv) < 2 or sys.argv[1] not in commands:
        sys.exit(__doc__)
    WORK.mkdir(parents=True, exist_ok=True)
    commands[sys.argv[1]](sys.argv[2] if len(sys.argv) > 2 else "")
