#!/usr/bin/env python3
"""The test programs linked by Scala Native's toolchain against lib-jar's jars (#168).

Usage: scripts/lib-link.py ours|published [FILTER]

Links each test program whose name contains FILTER (default: all) with Scala Native 0.5.12's
scala-native-ld and clang: its committed NIR and, as the whole class path, the jars of LINKED that
`just lib-jar` wrote into tmp/lib/MODULE (`ours`) or the published ones (`published`). Runs the
binary and compares its output with the `.check` file; that of a program with a `.check-by-hand`
(written for the VM) and its exit code with the binary's linked against the published jars by an
earlier `published` run, "by hand" without one. Fails before linking when the class path holds
another jar or the linker's own class path holds a library jar. Writes
tmp/lib-link/MODE/results.tsv, prints the programs neither passed nor by hand and one summary
line, and exits with 1 if there are any.
"""
import collections
import concurrent.futures
import importlib
import os
import queue
import re
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
# The errors of the linker's jar file systems closed while in use, which a retry links past.
SPURIOUS = ["ClosedFileSystemException", '"this.inodes" is null']
# Times a link is tried while the linker fails with one of SPURIOUS.
LINK_ATTEMPTS = 5
# The line of a run log giving the exit code.
EXIT = "--- exit "


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
    failed = link(name, jars, workdirs, out)
    if failed:
        return "link failed", failed
    try:
        r = subprocess.run([str(out / "bin" / name)], cwd=PROGRAMS / name, stdout=subprocess.PIPE,
                           stderr=subprocess.STDOUT, text=True, errors="replace", timeout=TIMEOUT)
    except subprocess.TimeoutExpired:
        return "timeout", ""
    stdout = re.sub(r"si_addr=0x[0-9a-f]+", "si_addr=ADDRESS", r.stdout.rstrip())
    got = f"{stdout}\n{EXIT}{r.returncode}\n"
    (out / "logs" / f"{name}.out").write_text(got)
    if (PROGRAMS / name / ".check-by-hand").exists():
        published = WORK / "published" / "logs" / f"{name}.out"
        if out.name == "published" or not published.exists():
            return "by hand", f"exit {r.returncode}"
        return ("passed", "") if got == published.read_text() else ("wrong output", "unlike the published jars'")
    if r.returncode != 0:
        return "crashed", f"exit {r.returncode}"
    if r.stdout.rstrip("\n") != check.read_text(errors="replace").rstrip("\n"):
        return "wrong output", ""
    return "passed", ""


def link(name, jars, workdirs, out):
    """Links NAME into out/bin, retrying the linker's failures of SPURIOUS; the first error line
    on failure, else none."""
    workdir = workdirs.get()
    try:
        cmd = [*LD, "--main", name, "-o", str(out / "bin" / name), "--workdir", str(workdir), str(PROGRAMS / name),
               *map(str, jars)]
        for _ in range(LINK_ATTEMPTS):
            r = subprocess.run(cmd, capture_output=True, text=True, errors="replace")
            log = r.stdout + r.stderr
            (out / "logs" / f"{name}.link").write_text(log)
            if r.returncode == 0:
                return None
            if not any(s in log for s in SPURIOUS):
                break
        return next((l for l in log.splitlines() if "rror" in l or "Exception" in l), "")[:160]
    finally:
        workdirs.put(workdir)


def main(mode, filter_):
    jars = class_path(mode)
    check_linker()
    out = WORK / mode
    for d in ("bin", "logs"):
        (out / d).mkdir(parents=True, exist_ok=True)
    workdirs = queue.Queue()
    for i in range(JOBS):
        workdirs.put(WORK / "work" / mode / str(i))
    names = sorted(p.name for p in PROGRAMS.iterdir() if p.is_dir() and filter_ in p.name)
    with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
        rows = list(pool.map(lambda n: link_run(n, jars, workdirs, out), names))
    (out / "results.tsv").write_text("".join(f"{n}\t{o}\t{d}\n" for n, (o, d) in zip(names, rows)))
    for name, (outcome, detail) in zip(names, rows):
        if outcome not in ("passed", "by hand"):
            print(f"{outcome:13} {name} {detail}")
    counts = collections.Counter(o for o, _ in rows)
    print(f"{mode}: {counts['passed']} of {len(names)} programs linked by scala-native-ld {lib.VERSION} with "
          f"outputs as expected; {dict(counts)}; class path {', '.join(os.path.relpath(j, lib.ROOT) for j in jars)}")
    sys.exit(0 if counts["passed"] + counts["by hand"] == len(names) else 1)


if __name__ == "__main__":
    if len(sys.argv) not in (2, 3) or sys.argv[1] not in ("ours", "published"):
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2] if len(sys.argv) == 3 else "")
