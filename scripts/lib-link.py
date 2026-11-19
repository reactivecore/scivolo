#!/usr/bin/env python3
"""The test programs linked by Scala Native's toolchain against lib-jar's jars (#168).

Usage: scripts/lib-link.py ours|published [FILTER]

Links each test program whose name contains FILTER (default: all) with Scala Native 0.5.12's
scala-native-ld and clang: its committed NIR and, as the whole class path, the jars of LINKED that
`just lib-jar` wrote into tmp/lib/MODULE (`ours`) or the published ones (`published`). Runs the
binary and compares its output with the `.check` file; that of a program with a `.check-by-hand`
(written for the VM) and its exit code with the binary's linked against the published jars by an
earlier `published` run ("by hand" there, "unchecked" without that run). Erases the signal address
in the output of ADDRESS_ERASED. Fails before linking when the class path holds a link or, in
`ours`, a published jar, or the linker's own class path holds a library jar. Retries a link at
most LINK_ATTEMPTS times while it fails with one of SPURIOUS and reports the retries. Writes
tmp/lib-link/MODE/results.tsv, prints the programs neither passed nor by hand and one summary
line, and exits with 1 if there are any.
"""
import collections
import concurrent.futures
import os
import queue
import re
import subprocess
import sys

import native_modules as lib

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
# The programs whose output shows the address of a signal, which differs between links.
ADDRESS_ERASED = {"overflow"}
# The line of a run log giving the exit code.
EXIT = "--- exit "


def class_path(mode):
    """The jars of LINKED for `mode`; exits unless each is a file of the directory of `mode`, not a
    link, and in `ours` not the published jar."""
    jars = [(lib.WORK / m if mode == "ours" else lib.LIBS) / lib.jar_name(m) for m in LINKED]
    for jar in jars:
        if not jar.is_file() or jar.is_symlink():
            sys.exit(f"{jar}: missing or a link (`just lib-jar {jar.parent.name}`)")
        if mode == "ours" and jar.read_bytes() == (lib.LIBS / jar.name).read_bytes():
            sys.exit(f"{jar}: the published jar")
    return jars


def check_linker():
    """Exits when the linker's own class path holds a jar of Scala Native's library."""
    r = subprocess.run(["coursier", "fetch", CLI], capture_output=True, text=True, check=True)
    found = [j for j in r.stdout.split() if "_native0.5_" in j]
    if found:
        sys.exit(f"scala-native-ld's class path holds library jars: {found}")


def link_run(name, jars, workdirs, out):
    """NAME linked and run: its outcome (passed, link failed, crashed, timeout, wrong output,
    by hand, unchecked or no .check), a detail and the number of link retries."""
    check = PROGRAMS / name / f"{name}.check"
    if not check.exists():
        return "no .check", "", 0
    (out / "logs" / f"{name}.out").unlink(missing_ok=True)
    failed, retries = link(name, jars, workdirs, out)
    if failed is not None:
        return "link failed", failed, retries
    try:
        r = subprocess.run([str(out / "bin" / name)], cwd=PROGRAMS / name, stdout=subprocess.PIPE,
                           stderr=subprocess.STDOUT, text=True, errors="replace", timeout=TIMEOUT)
    except subprocess.TimeoutExpired:
        return "timeout", "", retries
    stdout = r.stdout.rstrip()
    if name in ADDRESS_ERASED:
        stdout = re.sub(r"si_addr=0x[0-9a-f]+", "si_addr=ADDRESS", stdout)
    got = f"{stdout}\n{EXIT}{r.returncode}\n"
    (out / "logs" / f"{name}.out").write_text(got)
    if (PROGRAMS / name / ".check-by-hand").exists():
        published = WORK / "published" / "logs" / f"{name}.out"
        if out.name == "published":
            return "by hand", f"exit {r.returncode}", retries
        if not published.exists():
            return "unchecked", "no output of `just lib-link published`", retries
        same = got == published.read_text()
        return ("passed", "", retries) if same else ("wrong output", "unlike the published jars'", retries)
    if r.returncode != 0:
        return "crashed", f"exit {r.returncode}", retries
    if r.stdout.rstrip("\n") != check.read_text(errors="replace").rstrip("\n"):
        return "wrong output", "", retries
    return "passed", "", retries


def link(name, jars, workdirs, out):
    """Links NAME into out/bin, retrying the linker's failures of SPURIOUS; the first error line
    on failure, else None, and the number of retries. The link log holds every attempt."""
    workdir = workdirs.get()
    try:
        cmd = [*LD, "--main", name, "-o", str(out / "bin" / name), "--workdir", str(workdir), str(PROGRAMS / name),
               *map(str, jars)]
        logs = []
        for attempt in range(LINK_ATTEMPTS):
            r = subprocess.run(cmd, capture_output=True, text=True, errors="replace")
            log = r.stdout + r.stderr
            logs.append(f"--- attempt {attempt + 1}, exit {r.returncode}\n{log}")
            (out / "logs" / f"{name}.link").write_text("".join(logs))
            if r.returncode == 0:
                return None, attempt
            if not any(s in log for s in SPURIOUS):
                break
        return next((l for l in log.splitlines() if "rror" in l or "Exception" in l), "")[:160], attempt
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
    (out / "results.tsv").write_text("".join(f"{n}\t{o}\t{d}\t{t}\n" for n, (o, d, t) in zip(names, rows)))
    for name, (outcome, detail, retries) in zip(names, rows):
        if outcome not in ("passed", "by hand"):
            print(f"{outcome:13} {name} {detail}")
        if retries:
            print(f"{'retried':13} {name} {retries} times past {' or '.join(SPURIOUS)}")
    counts = collections.Counter(o for o, _, _ in rows)
    retried = sum(1 for _, _, t in rows if t)
    print(f"{mode}: {counts['passed']} of {len(names)} programs linked by scala-native-ld {lib.VERSION} with "
          f"outputs as expected; {dict(counts)}; {retried} links retried; "
          f"class path {', '.join(os.path.relpath(j, lib.ROOT) for j in jars)}")
    sys.exit(0 if counts["passed"] + counts["by hand"] == len(names) else 1)


if __name__ == "__main__":
    if len(sys.argv) not in (2, 3) or sys.argv[1] not in ("ours", "published"):
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2] if len(sys.argv) == 3 else "")
