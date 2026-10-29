#!/usr/bin/env python3
"""dotty's tests/run through the Rust frontend: typer, lowering, then the VM.

Writes lower-report.md and lower-results.tsv under tmp/dotty/<versions>, beside the report of
the same tests compiled by Scala Native (`just dotty-report`). A test of several compilation
groups has its earlier groups compiled by scala-cli, which the lowering of the last group and
its run see on their classpath.

Usage: scripts/dotty-lower.py [FILTER] [DIR]

FILTER is a substring of the test names to run; their rows replace those of the last run. DIR
is the directory of dotty's tests (default: run); another one has no Scala Native outcomes to
compare with and writes lower-report-DIR.md and lower-results-DIR.tsv.
"""
import collections
import concurrent.futures
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

from dotty_sources import compile_groups, tests, using_options

SCALA, NATIVE = "3.9.0", "0.5.12"
ROOT = Path(__file__).resolve().parent.parent
WORK = ROOT / "tmp/dotty" / f"{SCALA}-{NATIVE}"
LOWER, SCIVOLO = ROOT / "target/release/scivolo-lower", ROOT / "target/release/scivolo"
JOBS = max(2, (os.cpu_count() or 4) // 2)
TIMEOUT = int(os.environ.get("SCIVOLO_TIMEOUT", "60"))
OUTCOMES = ["passed", "wrong output", "crashed", "timeout", "lowering refused", "typer refused",
            "parser refused", "macro refused", "frontend crashed", "earlier groups failed", "several files", "fails in the VM",
            "skipped"]
TOP = 40


def first_line(s):
    return next((l for l in s.splitlines() if l.strip()), "").replace("\t", " ")[:160]


def lines(s):
    return s.rstrip("\n").splitlines()


def earlier(name, groups, dir, options):
    """The scala-cli output of all groups but the last, compiled with `options` unless done, or None."""
    out = WORK / "earlier" / dir / name
    if not (out / ".done").exists():
        shutil.rmtree(out, ignore_errors=True)
        out.mkdir(parents=True)
        code, log = compile_groups(f"{dir}-{name}", groups[:-1], out, TIMEOUT, options)
        if code != 0:
            return None, first_line("\n".join(l for l in log.splitlines() if "error" in l) or log)
        (out / ".done").touch()
    return out, ""


def one(name, groups, check, reason, baseline, dir):
    """The outcome of test `name` through the frontend and the VM, and a detail; a group whose
    macro definitions the typer refuses compiles them first, with the options of the group, as
    dotty suspends their callers."""
    if reason:
        return "skipped", reason
    if dir == "run" and baseline != "passed":
        return "fails in the VM", baseline
    outcome = lowered(name, groups, check, dir)
    quoted = [s for s in groups[-1] if "scala.quoted" in s.read_text(errors="replace")]
    if outcome[0] == "typer refused" and 0 < len(quoted) < len(groups[-1]):
        rest = [s for s in groups[-1] if s not in quoted]
        return lowered(name, [*groups[:-1], quoted, rest], check, dir, using_options(groups[-1]))
    return outcome


def lowered(name, groups, check, dir, options=()):
    cp = []
    if len(groups) > 1:
        before, why = earlier(name, groups, dir, options)
        if before is None:
            return "earlier groups failed", why
        cp = ["--cp", str(before)]
    out = WORK / "lowered" / (name if dir == "run" else f"{dir}-{name}")
    try:
        r = subprocess.run([str(LOWER), *cp, *map(str, groups[-1]), str(out)], capture_output=True,
                           text=True, errors="replace", timeout=TIMEOUT)
    except subprocess.TimeoutExpired:
        return "frontend crashed", "timeout"
    if r.returncode != 0:
        if m := re.search(r"Failed to evaluate macro\..*?unsupported: no method (\S+)", r.stderr):
            return "macro refused", m[1].removeprefix("scala.quoted.runtime.impl.")
        message = first_line(r.stderr)
        if m := re.match(r"the (typer|lowering) does not support (.*?)( at (\S+:)?\d+:\d+)?$", message):
            return f"{m[1]} refused", m[2]
        if "syntax error" in message:
            return "parser refused", message
        return "frontend crashed", message
    env = dict(os.environ, SCIVOLO_TIMEOUT=str(TIMEOUT))
    try:
        r = subprocess.run([str(SCIVOLO), "--cp", str(out), *cp, "Test"], capture_output=True, text=True,
                           errors="replace", env=env, timeout=TIMEOUT + 10)
    except subprocess.TimeoutExpired:
        return "timeout", ""
    if "scivolo: timed out" in r.stderr:
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


def is_directory_test(groups, dir):
    """Whether the test of source `groups` is a directory of dotty's tests/`dir`, not a file."""
    return bool(groups and groups[0]) and groups[0][0].parent.name != dir


def report(filter_, dir):
    base = baselines() if dir == "run" else {}
    if dir == "run" and not base:
        sys.exit(f"{WORK / 'results.tsv'} missing: run `just dotty-report` first")
    todo = [(n, g, c, r, base.get(n, "not run"), dir) for n, g, c, r in tests(filter_, dir)]
    files = {t[0] for t in todo if not is_directory_test(t[1], dir)}
    todo = [(f"{t[0]}-dir" if t[0] in files and is_directory_test(t[1], dir) else t[0], *t[1:]) for t in todo]
    with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
        results = list(pool.map(lambda t: one(*t), todo))
    suffix = "" if dir == "run" else f"-{dir}"
    path = WORK / f"lower-results{suffix}.tsv"
    old = [l.split("\t") for l in path.read_text().splitlines()] if filter_ and path.exists() else []
    rows = {r[0]: (r[1], r[2]) for r in old} | {t[0]: r for t, r in zip(todo, results)}
    path.write_text("".join(f"{n}\t{o}\t{d}\n" for n, (o, d) in sorted(rows.items())))
    counts = collections.Counter(o for o, _ in rows.values())
    md = "| Outcome | Tests |\n|---|---:|\n" + "".join(f"| {o} | {counts[o]} |\n" for o in OUTCOMES)
    md += f"| **total** | **{len(rows)}** |\n"
    for stage in ("lowering", "typer", "macro"):
        reasons = collections.Counter(d for o, d in rows.values() if o == f"{stage} refused")
        md += f"\n### {stage.capitalize()} refusals (top {TOP} of {len(reasons)})\n\n| Tests | Reason |\n|---:|---|\n"
        md += "".join(f"| {c} | {r} |\n" for r, c in reasons.most_common(TOP))
    (WORK / f"lower-report{suffix}.md").write_text(md)
    print(md)


if __name__ == "__main__":
    report(sys.argv[1] if len(sys.argv) > 1 else "", sys.argv[2] if len(sys.argv) > 2 else "run")
