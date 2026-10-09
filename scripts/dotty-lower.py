#!/usr/bin/env python3
"""dotty's tests/run through the Rust frontend: typer, lowering, then the VM.

Writes lower-report.md and lower-results.tsv under tmp/dotty/<versions>, beside the report of
the same tests compiled by Scala Native (`just dotty-report`).

Usage: scripts/dotty-lower.py [FILTER]

FILTER is a substring of the test names to run; their rows replace those of the last run.
"""
import collections
import concurrent.futures
import os
import re
import subprocess
import sys
from pathlib import Path

from dotty_sources import tests

SCALA, NATIVE = "3.9.0", "0.5.12"
ROOT = Path(__file__).resolve().parent.parent
WORK = ROOT / "tmp/dotty" / f"{SCALA}-{NATIVE}"
LOWER, SCALAI = ROOT / "target/release/lower", ROOT / "target/release/scalai"
JOBS = max(2, (os.cpu_count() or 4) // 2)
TIMEOUT = int(os.environ.get("SCALAI_TIMEOUT", "60"))
OUTCOMES = ["passed", "wrong output", "crashed", "timeout", "lowering refused", "typer refused",
            "parser refused", "frontend crashed", "several files", "fails in the VM", "skipped"]
TOP = 40


def first_line(s):
    return next((l for l in s.splitlines() if l.strip()), "").replace("\t", " ")[:160]


def lines(s):
    return s.rstrip("\n").splitlines()


def one(name, groups, check, reason, baseline):
    """The outcome of test `name` through the frontend and the VM, and a detail."""
    if reason:
        return "skipped", reason
    sources = [s for g in groups for s in g]
    if len(sources) != 1:
        return "several files", ""
    if baseline != "passed":
        return "fails in the VM", baseline
    out = WORK / "lowered" / name
    try:
        r = subprocess.run([str(LOWER), str(sources[0]), str(out)], capture_output=True, text=True,
                           errors="replace", timeout=TIMEOUT)
    except subprocess.TimeoutExpired:
        return "frontend crashed", "timeout"
    if r.returncode != 0:
        message = first_line(r.stderr)
        if m := re.match(r"the (typer|lowering) does not support (.*?)( at \d+:\d+)?$", message):
            return f"{m[1]} refused", m[2]
        if "syntax error" in message:
            return "parser refused", message
        return "frontend crashed", message
    env = dict(os.environ, SCALAI_TIMEOUT=str(TIMEOUT))
    try:
        r = subprocess.run([str(SCALAI), "--cp", str(out), "Test"], capture_output=True, text=True,
                           errors="replace", env=env, timeout=TIMEOUT + 10)
    except subprocess.TimeoutExpired:
        return "timeout", ""
    if "scalai: timed out" in r.stderr:
        return "timeout", ""
    if r.returncode != 0:
        return "crashed", first_line(r.stderr)
    if check and lines(r.stdout) != lines(check.read_text(errors="replace")):
        return "wrong output", ""
    return "passed", ""


def baselines():
    """The outcome of each test compiled by Scala Native, from `just dotty-report`."""
    path = WORK / "results.tsv"
    rows = [l.split("\t") for l in path.read_text().splitlines()] if path.exists() else []
    return {r[0]: r[1] for r in rows}


def report(filter_):
    base = baselines()
    if not base:
        sys.exit(f"{WORK / 'results.tsv'} missing: run `just dotty-report` first")
    todo = [(n, g, c, r, base.get(n, "not run")) for n, g, c, r in tests(filter_)]
    with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
        results = list(pool.map(lambda t: one(*t), todo))
    path = WORK / "lower-results.tsv"
    old = [l.split("\t") for l in path.read_text().splitlines()] if filter_ and path.exists() else []
    rows = {r[0]: (r[1], r[2]) for r in old} | {t[0]: r for t, r in zip(todo, results)}
    (WORK / "lower-results.tsv").write_text("".join(f"{n}\t{o}\t{d}\n" for n, (o, d) in sorted(rows.items())))
    counts = collections.Counter(o for o, _ in rows.values())
    md = "| Outcome | Tests |\n|---|---:|\n" + "".join(f"| {o} | {counts[o]} |\n" for o in OUTCOMES)
    md += f"| **total** | **{len(rows)}** |\n"
    for stage in ("lowering", "typer"):
        reasons = collections.Counter(d for o, d in rows.values() if o == f"{stage} refused")
        md += f"\n### {stage.capitalize()} refusals (top {TOP} of {len(reasons)})\n\n| Tests | Reason |\n|---:|---|\n"
        md += "".join(f"| {c} | {r} |\n" for r, c in reasons.most_common(TOP))
    (WORK / "lower-report.md").write_text(md)
    print(md)


if __name__ == "__main__":
    report(sys.argv[1] if len(sys.argv) > 1 else "")
