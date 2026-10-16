#!/usr/bin/env python3
"""Dotty's tests/run that pass in the VM, built as native executables with scalai-aot (#113).

Usage: scripts/aot-dotty.py [N|NAMES]   all VM-passing tests, an evenly spaced sample of N, or
                                        the comma-separated NAMES

Reads the NIR and the VM's results.tsv of `just dotty-build` / `just dotty-report`, in this
checkout's tmp/dotty or else the main checkout's; writes tmp/aot/dotty/results.tsv (test, status,
reason) and prints the counts and the most frequent failure reasons.
"""
import collections
import concurrent.futures
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MAIN = Path(subprocess.check_output(["git", "rev-parse", "--git-common-dir"], cwd=ROOT, text=True).strip())
MAIN = (ROOT / MAIN).resolve().parent
DOTTY = "tmp/dotty/3.9.0-0.5.12"
WORK = ROOT / DOTTY if (ROOT / DOTTY).is_dir() else MAIN / DOTTY
RUN = "ref/scala3/tests/run"
CHECKS = ROOT / RUN if (ROOT / RUN).is_dir() else MAIN / RUN
AOT = ROOT / "target/release/scalai-aot"
OUT = ROOT / "tmp/aot/dotty"
JOBS, TIMEOUT = 4, 20


def run(cmd):
    """Exit code (None on timeout), stdout and stderr of `cmd`."""
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, errors="replace", timeout=TIMEOUT)
        return r.returncode, r.stdout, r.stderr
    except subprocess.TimeoutExpired as e:
        out = e.stdout.decode(errors="replace") if isinstance(e.stdout, bytes) else e.stdout or ""
        return None, out, ""


def first_line(text):
    return next((l.strip() for l in text.splitlines() if l.strip()), "").replace("\t", " ")[:200]


def one(name):
    binary = OUT / "bin" / name
    code, _, err = run([str(AOT), "--cp", str(WORK / "nir" / name), "-o", str(binary), "Test"])
    if code is None:
        return "compile timeout", ""
    if code != 0:
        return "compile failed", first_line(err)
    code, out, err = run([str(binary)])
    (OUT / "logs" / f"{name}.out").write_text(out + "\n--- stderr ---\n" + err)
    if code is None:
        return "timeout", ""
    if code != 0:
        return "crashed", first_line(err) or f"exit {code}"
    check = CHECKS / f"{name}.check"
    if check.exists() and out.rstrip("\n").splitlines() != check.read_text(errors="replace").rstrip("\n").splitlines():
        return "wrong output", ""
    return "passed", ""


def group(reason):
    """A failure reason without the names and numbers of one test."""
    reason = re.sub(r"@?[\w$.]+::[^:]*?(?=:|$)", "<member>", reason)
    reason = re.sub(r"`[^`]*`|'[^']*'|\"[^\"]*\"", "<name>", reason)
    reason = re.sub(r"\b[\w$]+(\.[\w$]+)+\b", "<name>", reason)
    return re.sub(r"\d+", "<n>", reason)[:120]


def main():
    vm = [l.split("\t") for l in (WORK / "results.tsv").read_text().splitlines()]
    passing = sorted(r[0] for r in vm if len(r) > 1 and r[1] == "passed")
    arg = sys.argv[1] if len(sys.argv) > 1 else "0"
    n = int(arg) if arg.isdigit() else 0
    todo = [passing[i * len(passing) // n] for i in range(n)] if 0 < n < len(passing) else passing
    todo = todo if arg.isdigit() else [t for t in passing if t in arg.split(",")]
    (OUT / "bin").mkdir(parents=True, exist_ok=True)
    (OUT / "logs").mkdir(exist_ok=True)
    with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
        rows = dict(zip(todo, pool.map(one, todo)))
    (OUT / "results.tsv").write_text("".join(f"{t}\t{s}\t{r}\n" for t, (s, r) in sorted(rows.items())))
    counts = collections.Counter(s for s, _ in rows.values())
    print(f"aot: {counts['passed']} of {len(todo)} VM-passing tests ({len(passing)} pass in the VM)")
    print(dict(counts))
    reasons = collections.Counter(f"{s}: {group(r)}" for s, r in rows.values() if s != "passed")
    for reason, count in reasons.most_common(20):
        print(f"{count:4}  {reason}")


if __name__ == "__main__":
    main()
