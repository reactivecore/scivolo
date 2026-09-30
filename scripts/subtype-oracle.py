#!/usr/bin/env python3
"""The subtype oracle: dotc 3.9.0 decides `A <: B` for every pair of the types in
crates/tasty/tests/fixtures/subtype-types.txt, written to subtype-oracle.tsv beside it. Each
pair is a bounds check of `sub[A, B]` with `def sub[A, B >: A]`: subtyping alone, without the
inference and implicit search of `summon[A <:< B]`. The types may use the definitions of
fixtures/subtype/Prelude.scala, whose TASTy is packed into fixtures/subtype-prelude.jar.

Usage: scripts/subtype-oracle.py
"""
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "crates/tasty/tests/fixtures"
PRELUDE = FIXTURES / "subtype/Prelude.scala"
WORK = ROOT / "tmp/subtype-oracle"
CHUNK = 150


def run(cmd):
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"{' '.join(cmd)} failed\n{r.stdout[-2000:]}{r.stderr[-4000:]}")
    return r.stdout


def prelude_jar():
    """Compiles the prelude and packs its TASTy, entries sorted and dated alike."""
    out = WORK / "prelude"
    shutil.rmtree(out, ignore_errors=True)
    run(["scala-cli", "compile", "--server=false", "--quiet", "--scala", "3.9.0", "-d", str(out), str(PRELUDE)])
    with zipfile.ZipFile(FIXTURES / "subtype-prelude.jar", "w", zipfile.ZIP_DEFLATED) as jar:
        for path in sorted(out.rglob("*.tasty")):
            info = zipfile.ZipInfo(str(path.relative_to(out)), date_time=(2010, 1, 1, 0, 0, 0))
            jar.writestr(info, path.read_bytes(), zipfile.ZIP_DEFLATED)


def main():
    WORK.mkdir(parents=True, exist_ok=True)
    prelude_jar()
    types = [t for t in (FIXTURES / "subtype-types.txt").read_text().splitlines() if t.strip()]
    pairs = [(a, b) for a in types for b in types]
    chunks = [pairs[i:i + CHUNK] for i in range(0, len(pairs), CHUNK)]
    lines = ["import scala.compiletime.testing.typeChecks", "", "def sub[A, B >: A]: Unit = ()", ""]
    for n, chunk in enumerate(chunks):
        lines.append(f"def chunk{n}(): Unit =")
        for a, b in chunk:
            code = f"sub[{a}, {b}]".replace('"', '\\"')
            lines.append(f'  println(typeChecks("{code}"))')
    lines += ["", "@main def decide(): Unit ="] + [f"  chunk{n}()" for n in range(len(chunks))]
    (WORK / "Oracle.scala").write_text("\n".join(lines) + "\n")
    out = run(["scala-cli", "run", "--server=false", "--quiet", "--scala", "3.9.0", str(WORK / "Oracle.scala"), str(PRELUDE)])
    results = out.split()
    if len(results) != len(pairs):
        sys.exit(f"{len(results)} of {len(pairs)} results\n{out[-2000:]}")
    rows = [f"{a}\t{b}\t{res}" for (a, b), res in zip(pairs, results)]
    (FIXTURES / "subtype-oracle.tsv").write_text("\n".join(rows) + "\n")
    print(f"{len(rows)} pairs, {results.count('true')} subtypes")


if __name__ == "__main__":
    main()
