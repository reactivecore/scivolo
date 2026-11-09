#!/usr/bin/env python3
"""tests/programs through the Rust frontend and the JVM backend, run on `java -Xverify:all` and
compared with their `.check` (Scala Native's output, stdout and stderr together).

Usage: scripts/programs_jvm.py [FILTER]

Writes programs-results.tsv and programs-report.md under tmp/jvm; FILTER is a substring of the
program names to run, whose rows replace those of the last run.
"""
import concurrent.futures
import sys

from dotty_jvm import JOBS, LIBRARY, ROOT, TIMEOUT, ran, run, scivolo, write

PROGRAMS = ROOT / "tests/programs"
WORK = ROOT / "tmp/jvm"


def one(name):
    source, check = PROGRAMS / name / f"{name}.scala", PROGRAMS / name / f"{name}.check"
    if not check.exists():
        return "skipped", "no .check"
    out = scivolo(name, [source], [], [], WORK / "programs" / name)
    if isinstance(out, tuple):
        return out
    cmd = ["java", "-Xverify:all", "-Duser.language=en", "-Duser.country=US", "-cp", f"{out}:{LIBRARY}", name]
    code, stdout, err = run(cmd, TIMEOUT, cwd=PROGRAMS / name)
    (WORK / "programs-logs" / name).write_text(stdout + "\n--- stderr ---\n" + err)
    return ran(code, stdout + err, err, check)


def main(filter_):
    (WORK / "programs-logs").mkdir(parents=True, exist_ok=True)
    names = sorted(p.name for p in PROGRAMS.iterdir() if p.is_dir() and filter_ in p.name)
    with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
        results = list(pool.map(one, names))
    write(WORK / "programs-results.tsv", WORK / "programs-report.md", filter_, dict(zip(names, results)))


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "")
