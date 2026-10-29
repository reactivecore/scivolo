#!/usr/bin/env python3
"""The library overlay's committed NIR (ADR 0009), compiled from overlay/src with Scala Native.

Usage: scripts/overlay.py build   write the overlay/nir files that differ from a rebuild
       scripts/overlay.py stale   list overlay NIR files that differ from a rebuild

Files differ by their decoded definitions with positions, in any order: Scala Native orders
forwarders and members differently from run to run.
"""
import filecmp
import shutil
import subprocess
import sys
from pathlib import Path

from programs import NATIVE, ROOT, definitions

SOURCES = ROOT / "overlay/src"
NIR = ROOT / "overlay/nir"
WORK = ROOT / "tmp/overlay"


def compile_all():
    """Compiles a copy of overlay/src in tmp/overlay/src into tmp/overlay/out."""
    shutil.rmtree(WORK, ignore_errors=True)
    shutil.copytree(SOURCES, WORK / "src")
    cmd = ["scala-cli", "--power", "compile", "--quiet", "--server=false", "--native", "--native-version", NATIVE,
           "--scala", "3.9.0", "--workspace", str(WORK), "-d", str(WORK / "out"), "src"]
    r = subprocess.run(cmd, capture_output=True, text=True, cwd=WORK)
    if r.returncode != 0:
        sys.exit(f"scala-cli failed\n{r.stdout}{r.stderr}")
    return WORK / "out"


def nir(directory):
    return {str(p.relative_to(directory)) for p in directory.rglob("*.nir")}


def differences(out):
    """The NIR files a rebuild in OUT adds, removes and changes, by definitions and positions in any order."""
    added, removed = sorted(nir(out) - nir(NIR)), sorted(nir(NIR) - nir(out))
    _, differ, errors = filecmp.cmpfiles(NIR, out, sorted(nir(out) & nir(NIR)), shallow=False)
    changed = [f for f in differ if definitions(NIR / f, "--debug") != definitions(out / f, "--debug")]
    return added, removed, changed + errors


def build():
    out = compile_all()
    added, removed, changed = differences(out)
    for rel in removed:
        (NIR / rel).unlink()
    for rel in added + changed:
        (NIR / rel).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(out / rel, NIR / rel)


def stale():
    added, removed, changed = differences(compile_all())
    found = [f"+{f}" for f in added] + [f"-{f}" for f in removed] + changed
    print(" ".join(["STALE overlay", *found]) if found else "ok    overlay")
    sys.exit(1 if found else 0)


if __name__ == "__main__":
    commands = {"build": build, "stale": stale}
    if len(sys.argv) != 2 or sys.argv[1] not in commands:
        sys.exit(__doc__)
    commands[sys.argv[1]]()
