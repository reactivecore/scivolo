#!/usr/bin/env python3
"""The library overlay's committed NIR (ADR 0009), compiled from overlay/src with Scala Native.

Usage: scripts/overlay.py build   write overlay/nir
       scripts/overlay.py stale   list overlay NIR files that differ from a rebuild
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
    """Compiles overlay/src into tmp/overlay/out."""
    shutil.rmtree(WORK, ignore_errors=True)
    cmd = ["scala-cli", "--power", "compile", "--quiet", "--native", "--native-version", NATIVE, "--scala", "3.9.0",
           "--workspace", str(WORK), "-d", str(WORK / "out"), str(SOURCES)]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"scala-cli failed\n{r.stdout}{r.stderr}")
    return WORK / "out"


def nir(directory):
    return {str(p.relative_to(directory)) for p in directory.rglob("*.nir")}


def build():
    out = compile_all()
    shutil.rmtree(NIR, ignore_errors=True)
    for rel in nir(out):
        (NIR / rel).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(out / rel, NIR / rel)


def stale():
    out = compile_all()
    added, removed = sorted(nir(out) - nir(NIR)), sorted(nir(NIR) - nir(out))
    common = sorted(nir(out) & nir(NIR))
    _, differ, errors = filecmp.cmpfiles(NIR, out, common, shallow=False)
    changed = [f for f in differ if definitions(NIR / f) != definitions(out / f)] + errors
    found = [f"+{f}" for f in added] + [f"-{f}" for f in removed] + changed
    print(" ".join(["STALE overlay", *found]) if found else "ok    overlay")
    sys.exit(1 if found else 0)


if __name__ == "__main__":
    commands = {"build": build, "stale": stale}
    if len(sys.argv) != 2 or sys.argv[1] not in commands:
        sys.exit(__doc__)
    commands[sys.argv[1]]()
