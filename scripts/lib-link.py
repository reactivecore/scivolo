#!/usr/bin/env python3
"""The test programs linked by Scala Native's toolchain against lib-jar's jars (#168).

Usage: scripts/lib-link.py ours|published [FILTER]

Links each test program whose name contains FILTER (default: all) with Scala Native 0.5.12's
scala-native-ld and clang: its committed NIR and, as the whole class path, the jars of LINKED that
`just lib-jar` wrote into tmp/lib/MODULE (`ours`) or the published ones (`published`). Runs the
binary and compares its output with the `.check` file. Fails before linking when the class path
holds another jar or the linker's own class path holds a library jar. Writes
tmp/lib-link/MODE/results.tsv, prints the programs not passed and one summary line, and exits
with 1 unless all passed.
"""
import collections
import concurrent.futures
import importlib
import os
import queue
import shutil
import subprocess
import sys

lib = importlib.import_module("lib-compile")

CLI = "org.scala-native:scala-native-cli_2.13:" + lib.VERSION
LD = ["coursier", "launch", CLI, "-M", "scala.scalanative.cli.ScalaNativeLd", "--"]
# The modules on an application's class path in Scala Native's build, those of scalalib's POM.
LINKED = ["nativelib", "clib", "posixlib", "windowslib", "javalib", "auxlib", "scalalib"]
PROGRAMS = lib.ROOT / "tests/programs"
WORK = lib.ROOT / "tmp/lib-link"
JOBS = max(2, (os.cpu_count() or 4) // 2)
# Seconds a linked program may run.
TIMEOUT = 60


def class_path(mode):
    """The jars of LINKED for `mode`; exits unless each is there and in the directory of `mode`."""
    jars = [(lib.WORK / m if mode == "ours" else lib.LIBS) / lib.jar_name(m) for m in LINKED]
    home = lib.WORK if mode == "ours" else lib.LIBS
    for jar in jars:
        if not jar.is_file() or home.resolve() not in jar.resolve().parents:
            sys.exit(f"{jar}: missing or outside {home} (`just lib-jar {jar.parent.name}`)")
    return jars


def check_linker():
    """Exits when the linker's own class path holds a jar of Scala Native's library."""
    r = subprocess.run(["coursier", "fetch", CLI], capture_output=True, text=True, check=True)
    found = [j for j in r.stdout.split() if "_native0.5_" in j]
    if found:
        sys.exit(f"scala-native-ld's class path holds library jars: {found}")


def link_run(name, jars, workdirs, out):
    """NAME linked and run: passed, link failed, crashed, timeout, wrong output or no .check."""
    check = PROGRAMS / name / f"{name}.check"
    if not check.exists():
        return "no .check", ""
    binary, workdir = out / "bin" / name, workdirs.get()
    try:
        cmd = [*LD, "--main", name, "-o", str(binary), "--workdir", str(workdir), str(PROGRAMS / name), *map(str, jars)]
        r = subprocess.run(cmd, capture_output=True, text=True, errors="replace")
        (out / "logs" / f"{name}.link").write_text(r.stdout + r.stderr)
        if r.returncode != 0:
            return "link failed", next((l for l in (r.stdout + r.stderr).splitlines() if "rror" in l), "")[:160]
    finally:
        workdirs.put(workdir)
    try:
        r = subprocess.run([str(binary)], cwd=PROGRAMS / name, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                           text=True, errors="replace", timeout=TIMEOUT)
    except subprocess.TimeoutExpired:
        return "timeout", ""
    (out / "logs" / f"{name}.out").write_text(r.stdout)
    if r.returncode != 0:
        return "crashed", f"exit {r.returncode}"
    if r.stdout.rstrip("\n") != check.read_text(errors="replace").rstrip("\n"):
        return "wrong output", "check by hand" if (PROGRAMS / name / ".check-by-hand").exists() else ""
    return "passed", ""


def main(mode, filter_):
    jars = class_path(mode)
    check_linker()
    out = WORK / mode
    shutil.rmtree(out, ignore_errors=True)
    for d in ("bin", "logs"):
        (out / d).mkdir(parents=True)
    workdirs = queue.Queue()
    for i in range(JOBS):
        workdirs.put(WORK / "work" / mode / str(i))
    names = sorted(p.name for p in PROGRAMS.iterdir() if p.is_dir() and filter_ in p.name)
    with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
        rows = list(pool.map(lambda n: link_run(n, jars, workdirs, out), names))
    (out / "results.tsv").write_text("".join(f"{n}\t{o}\t{d}\n" for n, (o, d) in zip(names, rows)))
    for name, (outcome, detail) in zip(names, rows):
        if outcome != "passed":
            print(f"{outcome:13} {name} {detail}")
    counts = collections.Counter(o for o, _ in rows)
    print(f"{mode}: {counts['passed']} of {len(names)} programs linked by scala-native-ld {lib.VERSION} with "
          f"outputs as their .check; {dict(counts)}; class path {', '.join(os.path.relpath(j, lib.ROOT) for j in jars)}")
    sys.exit(0 if counts["passed"] == len(names) else 1)


if __name__ == "__main__":
    if len(sys.argv) not in (2, 3) or sys.argv[1] not in ("ours", "published"):
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2] if len(sys.argv) == 3 else "")
