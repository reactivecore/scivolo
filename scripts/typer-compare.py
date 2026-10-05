#!/usr/bin/env python3
"""Types Scala files with dotty's oracle and with the Rust typer, and prints per file whether the
typed trees are the same, why the Rust typer fell back, or where they first differ.

Usage: scripts/typer-compare.py FILE...
"""
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORK = ROOT / "tmp/typer/compare"
DUMP = ROOT / "target/release/typer-dump"


def records(text):
    out = {}
    for chunk in text.split("== file ")[1:]:
        path, _, body = chunk.partition("\n")
        out[path.strip()] = body.split("\n")[0]
    return out


def main(files):
    WORK.mkdir(parents=True, exist_ok=True)
    paths = [str(Path(f).resolve()) for f in files]
    lst = WORK / "list.tsv"
    lst.write_text("".join(f"{p}\t\n" for p in paths))
    oracle = subprocess.run(["scala-cli", "--power", "run", "--server=false", "--quiet", "--workspace",
                             str(ROOT / "tmp/typer/3.9.0/oracle-ws"), str(ROOT / "scripts/typer-oracle"), "--",
                             "--list", str(lst)], capture_output=True, text=True)
    try:
        ours = subprocess.run([str(DUMP), "--list", str(lst)], capture_output=True, text=True, timeout=120)
        ours_out = ours.stdout
    except subprocess.TimeoutExpired:
        ours_out = ""
    expected, actual = records(oracle.stdout), records(ours_out)
    for p in paths:
        e, r = expected.get(p, "== no oracle output"), actual.get(p, "== no output (crash or timeout)")
        name = Path(p).name
        if e == r:
            print(f"{name}: same")
        elif r.startswith("=="):
            print(f"{name}: {r}")
        else:
            i = next((k for k in range(min(len(e), len(r))) if e[k] != r[k]), min(len(e), len(r)))
            print(f"{name}: differs at {i}\n  dotty: {e[max(0, i - 200):i + 150]}\n  ours:  {r[max(0, i - 200):i + 150]}")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    main(sys.argv[1:])
