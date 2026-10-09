#!/usr/bin/env python3
"""The oracles' outcomes, recorded under tmp/snapshot before a refactor and compared after it.

Usage: scripts/snapshot.py record|check [ORACLE...]

An oracle whose inputs are missing locally is skipped; `check` fails on any outcome that differs
from the snapshot, and on an oracle that runs but has no snapshot.
"""
import concurrent.futures
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SNAPSHOT = ROOT / "tmp/snapshot"
PROGRAMS = ROOT / "tests/programs"
SCALAI = ROOT / "target/release/scalai"
JOBS = max(2, (os.cpu_count() or 4) // 2)
TIMEOUT = int(os.environ.get("SCALAI_TIMEOUT", "60"))
SHOWN = 20
DOTTY = "tmp/dotty/3.9.0-0.5.12"


class Skipped(Exception):
    pass


class Failed(Exception):
    pass


def report(command, result, *needs):
    """An oracle that runs the report script `command` and reads its result file."""
    def run():
        missing = [n for n in needs if not (ROOT / n).exists()]
        if missing:
            raise Skipped(f"{', '.join(missing)} missing")
        r = subprocess.run(["python3", *command], cwd=ROOT, capture_output=True, text=True, errors="replace")
        if r.returncode != 0:
            raise Failed(f"{' '.join(command)} failed: {(r.stderr or r.stdout).strip()[-500:]}")
        return (ROOT / result).read_text(errors="replace")
    return run


def program(name):
    """Whether test program `name` prints its `.check` file in the release VM."""
    env = dict(os.environ, SCALAI_TIMEOUT=str(TIMEOUT))
    try:
        r = subprocess.run([str(SCALAI), "--cp", str(PROGRAMS / name), name], cwd=ROOT, env=env, text=True,
                           errors="replace", stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=TIMEOUT + 10)
    except subprocess.TimeoutExpired:
        return "timeout"
    check = (PROGRAMS / name / f"{name}.check").read_text(errors="replace")
    return "ok" if r.stdout.rstrip("\n") == check.rstrip("\n") else "differs"


def programs():
    names = sorted(p.name for p in PROGRAMS.iterdir() if p.is_dir())
    with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
        outcomes = list(pool.map(program, names))
    return "".join(f"{n}\t{o}\n" for n, o in zip(names, outcomes))


def syntax(mode):
    work = f"tmp/syntax/3.9.0/{mode}"
    return report(["scripts/syntax-tests.py", mode, "report"], f"{work}/results.tsv", "ref/dotty/tests",
                  f"{work}/expected.txt")


ORACLES = {
    "typer": report(["scripts/typer-tests.py", "report"], "tmp/typer/3.9.0/results.tsv", "ref/dotty/tests",
                    "tmp/typer/3.9.0/expected.txt"),
    "lower": report(["scripts/dotty-lower.py"], f"{DOTTY}/lower-results.tsv", "ref/scala3/tests/run",
                    f"{DOTTY}/results.tsv"),
    "tokens": syntax("tokens"),
    "trees": syntax("trees"),
    "desugar": syntax("desugar"),
    "programs": programs,
}


def rows(text):
    """{name: outcome and detail} of a result file."""
    return dict((l.split("\t", 1) + [""])[:2] for l in text.splitlines() if l)


def differences(before, after):
    """The lines describing each row that differs between two result files."""
    out = []
    for name in sorted(before.keys() | after.keys()):
        b, a = before.get(name, "missing"), after.get(name, "missing")
        if b != a:
            out.append(f"    {name}: {b.replace(chr(9), ' ')[:120]} -> {a.replace(chr(9), ' ')[:120]}")
    return out


def one(command, name):
    """Runs oracle `name`; its summary line, the lines of its differences, and whether it passed."""
    path = SNAPSHOT / f"{name}.tsv"
    start = time.monotonic()
    try:
        text = ORACLES[name]()
    except Skipped as e:
        if command == "record":
            path.unlink(missing_ok=True)
        return f"{name:<9} skipped: {e}", [], True
    except Failed as e:
        return f"{name:<9} FAILED: {e}", [], False
    took = f"{time.monotonic() - start:6.1f} s"
    count = len(rows(text))
    if command == "record":
        path.write_text(text)
        return f"{name:<9} recorded {count} rows  {took}", [], True
    if not path.exists():
        return f"{name:<9} FAILED: no snapshot: run `just snapshot {name}`", [], False
    diff = differences(rows(path.read_text(errors="replace")), rows(text))
    if not diff:
        return f"{name:<9} same     {count} rows  {took}", [], True
    shown = diff[:SHOWN] + ([f"    ... {len(diff) - SHOWN} more"] if len(diff) > SHOWN else [])
    return f"{name:<9} DIFFERS  {len(diff)} of {count} rows  {took}", shown, False


def main(command, names):
    unknown = [n for n in names if n not in ORACLES]
    if command not in ("record", "check") or unknown:
        sys.exit(__doc__ + f"\nORACLE = {' | '.join(ORACLES)}")
    SNAPSHOT.mkdir(parents=True, exist_ok=True)
    passed = True
    for name in names or ORACLES:
        summary, diff, ok = one(command, name)
        print(summary, *diff, sep="\n", flush=True)
        passed &= ok
    sys.exit(0 if passed else 1)


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "", sys.argv[2:])
