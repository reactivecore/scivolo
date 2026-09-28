#!/usr/bin/env python3
"""The test programs' committed NIR and `.check` files, compiled with Scala Native.

Usage: scripts/programs.py update NAME     write tests/programs/NAME's NIR and .check
       scripts/programs.py stale [NAME...]  list programs whose NIR or .check differs from a rebuild

A program with a `.check-by-hand` marker keeps its hand-written `.check`.
"""
import concurrent.futures
import filecmp
import os
import shutil
import subprocess
import sys
from pathlib import Path

NATIVE = "0.5.12"
ROOT = Path(__file__).resolve().parent.parent
PROGRAMS = ROOT / "tests/programs"
WORK = ROOT / "tmp/programs"
JOBS = max(2, (os.cpu_count() or 4) // 2)


def scala_cli(command, name, *extra):
    work = WORK / name
    return ["scala-cli", "--power", command, "--quiet", "--native", "--native-version", NATIVE,
            "--workspace", str(work), *extra, str(work / f"{name}.scala")]


def by_hand(name):
    return (PROGRAMS / name / ".check-by-hand").exists()


def build(name):
    """Compiles NAME's NIR into tmp/programs/NAME/out; the native binary's stdout unless checked by hand."""
    work = WORK / name
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True)
    shutil.copy(PROGRAMS / name / f"{name}.scala", work)
    cmds = [scala_cli("compile", name, "-d", str(work / "out"))]
    if not by_hand(name):
        cmds.append(scala_cli("package", name, "-f", "-o", str(work / "bin")))
    for cmd in cmds:
        r = subprocess.run(cmd, capture_output=True, text=True)
        if r.returncode != 0:
            sys.exit(f"{name}: scala-cli failed\n{r.stdout}{r.stderr}")
    return None if by_hand(name) else subprocess.run([str(work / "bin")], capture_output=True, text=True, timeout=600).stdout


def nir(directory):
    return {p.name for p in directory.glob("*.nir")}


def update(name):
    out = build(name)
    for f in (PROGRAMS / name).glob("*.nir"):
        f.unlink()
    for f in (WORK / name / "out").glob("*.nir"):
        shutil.copy(f, PROGRAMS / name)
    if out is not None:
        (PROGRAMS / name / f"{name}.check").write_text(out)


def differences(name):
    """What differs between NAME's committed files and a rebuild."""
    out = build(name)
    committed, built = PROGRAMS / name, WORK / name / "out"
    added, removed = nir(built) - nir(committed), nir(committed) - nir(built)
    _, changed, errors = filecmp.cmpfiles(committed, built, sorted(nir(committed) & nir(built)), shallow=False)
    found = [f"+{f}" for f in sorted(added)] + [f"-{f}" for f in sorted(removed)] + changed + errors
    if out is not None and out != (committed / f"{name}.check").read_text():
        found.append(f"{name}.check")
    return found


def stale(names):
    names = names or sorted(p.name for p in PROGRAMS.iterdir() if p.is_dir())
    with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
        found = dict(zip(names, pool.map(differences, names)))
    for name, files in found.items():
        print(f"{'STALE' if files else 'ok   '} {name} {' '.join(files)}")
    sys.exit(1 if any(found.values()) else 0)


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "update":
        update(sys.argv[2])
    elif len(sys.argv) >= 2 and sys.argv[1] == "stale":
        stale(sys.argv[2:])
    else:
        sys.exit(__doc__)
