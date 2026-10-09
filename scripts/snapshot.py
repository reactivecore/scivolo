#!/usr/bin/env python3
"""The oracles' outcomes, recorded under tmp/snapshot before a refactor and compared after it.

Usage: scripts/snapshot.py record|check [ORACLE...]

An oracle whose inputs are missing locally is skipped, and `record` keeps its old snapshot.
`check` fails on any outcome that differs from the snapshot, on an oracle that runs without a
snapshot or is skipped with one, and when it compared no oracle.
"""
import subprocess
import sys
import time
from pathlib import Path

import programs as test_programs

ROOT = Path(__file__).resolve().parent.parent
SNAPSHOT = ROOT / "tmp/snapshot"
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
        if not (ROOT / result).exists():
            raise Failed(f"{' '.join(command)} wrote no {result}")
        return (ROOT / result).read_text(errors="replace")
    return run


def programs():
    """Each test program's outcome in the release VM and the hash of its output."""
    try:
        return "".join("{}\t{}\t{}\n".format(n, *test_programs.run(n)) for n in test_programs.all_names())
    except test_programs.Failed as e:
        raise Failed(str(e))


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
    """Runs oracle `name`: its summary and difference lines, and `ok`, `skipped` or `failed`."""
    path = SNAPSHOT / f"{name}.tsv"
    start = time.monotonic()
    try:
        text = ORACLES[name]()
    except Skipped as e:
        if not path.exists():
            return [f"{name:<9} skipped: {e}"], "skipped"
        if command == "record":
            return [f"{name:<9} skipped: {e}; old snapshot kept"], "skipped"
        return [f"{name:<9} FAILED: skipped with a snapshot: {e}"], "failed"
    except Failed as e:
        kept = "; old snapshot kept" if command == "record" and path.exists() else ""
        return [f"{name:<9} FAILED: {e}{kept}"], "failed"
    took = f"{time.monotonic() - start:6.1f} s"
    count = len(rows(text))
    if command == "record":
        path.write_text(text)
        return [f"{name:<9} recorded {count} rows  {took}"], "ok"
    if not path.exists():
        return [f"{name:<9} FAILED: no snapshot: run `just snapshot {name}`"], "failed"
    diff = differences(rows(path.read_text(errors="replace")), rows(text))
    if not diff:
        return [f"{name:<9} same     {count} rows  {took}"], "ok"
    shown = diff[:SHOWN] + ([f"    ... {len(diff) - SHOWN} more"] if len(diff) > SHOWN else [])
    return [f"{name:<9} DIFFERS  {len(diff)} of {count} rows  {took}", *shown], "failed"


def main(command, names):
    unknown = [n for n in names if n not in ORACLES]
    if command not in ("record", "check") or unknown:
        sys.exit(__doc__ + f"\nORACLE = {' | '.join(ORACLES)}")
    SNAPSHOT.mkdir(parents=True, exist_ok=True)
    states = []
    for name in names or ORACLES:
        lines, state = one(command, name)
        print(*lines, sep="\n", flush=True)
        states.append(state)
    if "ok" not in states:
        print(f"FAILED: no oracle {'recorded' if command == 'record' else 'compared'}")
    sys.exit(0 if "ok" in states and "failed" not in states else 1)


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "", sys.argv[2:])
