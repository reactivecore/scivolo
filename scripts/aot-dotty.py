#!/usr/bin/env python3
"""Dotty's tests/run that pass in the VM, built as native executables with scivolo-aot (#113).

Usage: scripts/aot-dotty.py [N|NAMES]   all VM-passing tests, an evenly spaced sample of N, or
                                        the comma-separated NAMES

Reads the NIR and the VM's results.tsv of `just dotty-build` / `just dotty-report`; writes
tmp/aot/dotty/results.tsv (test, status, reason) and prints the counts and the most frequent
failure reasons. Compiles run one at a time (the compiler uses all cores), executables JOBS at a
time; each stops after AOT_TIMEOUT seconds (default 20).
"""
import collections
import concurrent.futures
import os
import re
import sys

from dotty_sources import NATIVE, ROOT, SCALA, WORK, lines, load, run, save, shared, tests

AOT = ROOT / "target/release/scivolo-aot"
NIR = shared(f"tmp/dotty/{SCALA}-{NATIVE}/nir")
OUT = ROOT / "tmp/aot/dotty"
JOBS, TIMEOUT = 4, int(os.environ.get("AOT_TIMEOUT", "20"))


def first_line(text):
    return next((l.strip() for l in text.splitlines() if l.strip()), "").replace("\t", " ")[:200]


def compile_one(name):
    """None when `name` compiled, else its status and reason."""
    binary, out = OUT / "bin" / name, OUT / "logs" / f"{name}.out"
    binary.unlink(missing_ok=True)
    out.unlink(missing_ok=True)
    code, stdout, stderr = run([str(AOT), "--cp", str(NIR / name), "-o", str(binary), "Test"], TIMEOUT)
    (OUT / "logs" / f"{name}.compile").write_text(stdout + "\n--- stderr ---\n" + stderr)
    if code is None:
        return "compile timeout", ""
    return None if code == 0 else ("compile failed", first_line(stderr))


def run_one(name, check):
    code, out, err = run([str(OUT / "bin" / name)], TIMEOUT)
    (OUT / "logs" / f"{name}.out").write_text(out + "\n--- stderr ---\n" + err)
    if code is None:
        return "timeout", ""
    if code != 0:
        return "crashed", first_line(err) or f"exit {code}"
    if check and lines(out) != lines(check.read_text(errors="replace")):
        return "wrong output", ""
    return "passed", ""


def group(reason):
    """A failure reason without the names and numbers of one test."""
    reason = re.sub(r"@?[\w$.]+::[^:]*?(?=:|$)", "<member>", reason)
    reason = re.sub(r"`[^`]*`|'[^']*'|\"[^\"]*\"", "<name>", reason)
    reason = re.sub(r"\b[\w$]+(\.[\w$]+)+\b", "<name>", reason)
    return re.sub(r"\d+", "<n>", reason)[:120]


def selected(passing, arg):
    """The tests to run: a sample of `arg` tests if it is a number, else the names it lists."""
    if arg.isdigit():
        n = int(arg)
        return [passing[i * len(passing) // n] for i in range(n)] if 0 < n < len(passing) else passing
    names = arg.split(",")
    if unknown := [n for n in names if n not in passing]:
        print(f"not passing in the VM, run anyway: {', '.join(unknown)}")
    return names


def main():
    if not (WORK / "results.tsv").exists():
        sys.exit(f"{WORK / 'results.tsv'} missing: run `just dotty-build` and `just dotty-report`")
    passing = sorted(n for n, (o, _) in load("results.tsv").items() if o == "passed")
    todo = selected(passing, sys.argv[1] if len(sys.argv) > 1 else "0")
    checks = {name: check for name, _, check, _ in tests("")}
    (OUT / "bin").mkdir(parents=True, exist_ok=True)
    (OUT / "logs").mkdir(exist_ok=True)
    rows, pool = {}, concurrent.futures.ThreadPoolExecutor(JOBS)
    try:
        runs = {}
        for name in todo:
            failed = compile_one(name)
            if failed:
                rows[name] = failed
            else:
                runs[name] = pool.submit(run_one, name, checks.get(name))
        rows |= {name: f.result() for name, f in runs.items()}
    finally:
        pool.shutdown(cancel_futures=True)
    save(OUT / "results.tsv", rows)
    counts = collections.Counter(s for s, _ in rows.values())
    print(f"aot: {counts['passed']} of {len(todo)} tests ({len(passing)} pass in the VM)")
    print(dict(counts))
    reasons = collections.Counter(f"{s}: {group(r)}" for s, r in rows.values() if s != "passed")
    for reason, count in reasons.most_common(20):
        print(f"{count:4}  {reason}")


if __name__ == "__main__":
    main()
