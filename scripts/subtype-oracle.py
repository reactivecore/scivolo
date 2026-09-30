#!/usr/bin/env python3
"""The subtype oracle: dotc 3.9.0 decides `A <: B` for every pair of the types in
crates/tasty/tests/fixtures/subtype-types.txt, written to subtype-oracle.tsv beside it. Each
pair is a bounds check of `sub[A, B]` with `def sub[A, B >: A]`: subtyping alone, without the
inference and implicit search of `summon[A <:< B]`.

Usage: scripts/subtype-oracle.py
"""
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "crates/tasty/tests/fixtures"
WORK = ROOT / "tmp/subtype-oracle"
CHUNK = 150


def main():
    types = [t for t in (FIXTURES / "subtype-types.txt").read_text().splitlines() if t.strip()]
    pairs = [(a, b) for a in types for b in types]
    chunks = [pairs[i:i + CHUNK] for i in range(0, len(pairs), CHUNK)]
    lines = ["import scala.compiletime.testing.typeChecks", "", "def sub[A, B >: A]: Unit = ()", ""]
    for n, chunk in enumerate(chunks):
        lines.append(f"def chunk{n}(): Unit =")
        for a, b in chunk:
            lines.append(f'  println(typeChecks("sub[{a}, {b}]"))')
    lines += ["", "@main def oracle(): Unit ="] + [f"  chunk{n}()" for n in range(len(chunks))]
    WORK.mkdir(parents=True, exist_ok=True)
    (WORK / "Oracle.scala").write_text("\n".join(lines) + "\n")
    cmd = ["scala-cli", "run", "--server=false", "--quiet", "--scala", "3.9.0", str(WORK / "Oracle.scala")]
    r = subprocess.run(cmd, capture_output=True, text=True)
    results = r.stdout.split()
    if r.returncode != 0 or len(results) != len(pairs):
        sys.exit(f"scala-cli failed ({len(results)} of {len(pairs)} results)\n{r.stdout[-2000:]}{r.stderr[-4000:]}")
    rows = [f"{a}\t{b}\t{res}" for (a, b), res in zip(pairs, results)]
    (FIXTURES / "subtype-oracle.tsv").write_text("\n".join(rows) + "\n")
    print(f"{len(rows)} pairs, {results.count('true')} subtypes")


if __name__ == "__main__":
    main()
