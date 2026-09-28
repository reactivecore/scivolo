#!/usr/bin/env python3
"""Runs dotty's tests/run (ref/scala3, tag 3.9.0) in the VM: milestone M4.

Usage: scripts/dotty-tests.py build [FILTER]   compile the Scala Native subset to NIR
       scripts/dotty-tests.py report [FILTER]  run it; summary in tmp/dotty/report.md,
                                              one line per test in tmp/dotty/results.tsv
       scripts/dotty-tests.py native [FILTER]  run the tests failing in the VM as Scala Native
                                              binaries; tmp/dotty/native.tsv, used by report
FILTER keeps tests whose name contains it. SCALAI_TIMEOUT (default 60) bounds each run.
"""
import concurrent.futures
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TESTS = ROOT / "ref/scala3/tests/run"
WORK = ROOT / "tmp/dotty"
SCALAI = ROOT / "target/release/scalai"
JOBS = max(2, (os.cpu_count() or 4) // 2)

# Source text that needs the JVM: runtime reflection, class loading, processes, serialization.
JVM_ONLY = re.compile(
    r"scala\.reflect\.runtime|java\.lang\.reflect|getDeclared|\.getMethods?\b|\.getFields?\b"
    r"|Class\.forName|ClassLoader|scala\.tools\.|dotty\.tools|scala\.quoted\.staging"
    r"|scala\.sys\.process|java\.lang\.invoke|javax\.|sun\.misc|com\.sun\.|java\.rmi|java\.beans"
    r"|java\.awt|java\.lang\.management|ObjectOutputStream|ObjectInputStream|getResource"
    r"|//> using target\.platform"
)


def tests(filter_):
    """(name, sources, check file or None, skip reason or None) for each test."""
    for path in sorted(TESTS.iterdir()):
        name = path.name if path.is_dir() else path.stem
        if not (path.is_dir() or path.suffix == ".scala") or filter_ not in name:
            continue
        sources = [s for s in sorted(path.rglob("*")) if s.is_file()] if path.is_dir() else [path]
        check = TESTS / (name + ".check")
        reason = None
        if any(s.suffix == ".java" for s in sources):
            reason = "java sources"
        elif any(JVM_ONLY.search(s.read_text(errors="replace")) for s in sources if s.suffix == ".scala"):
            reason = "jvm-only api"
        yield name, [s for s in sources if s.suffix == ".scala"], check if check.exists() else None, reason


def compile_one(name, sources):
    out, log = WORK / "nir" / name, WORK / "logs" / (name + ".compile")
    if (out / ".done").exists():
        return name, "compiled"
    cmd = ["scala-cli", "--power", "compile", "--quiet", "--native", "--native-version", "0.5.12",
           "--scala", "3.9.0", "--workspace", str(WORK / "ws" / name), "-d", str(out)] + [str(s) for s in sources]
    result = subprocess.run(cmd, capture_output=True, text=True)
    log.write_text(result.stdout + result.stderr)
    if result.returncode != 0:
        return name, "compile failed"
    (out / ".done").touch()
    return name, "compiled"


def run_one(name, check, cmd=None):
    env = dict(os.environ, SCALAI_TIMEOUT=os.environ.get("SCALAI_TIMEOUT", "60"))
    cmd = cmd or [str(SCALAI), "--cp", str(WORK / "nir" / name), "Test"]
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, errors="replace", env=env,
                                timeout=int(env["SCALAI_TIMEOUT"]) + 10)
    except subprocess.TimeoutExpired:
        return "timeout", ""
    log = name + (".run" if cmd[0] == str(SCALAI) else ".native")
    (WORK / "logs" / log).write_text(result.stdout + "\n--- stderr ---\n" + result.stderr)
    first_err = next((l for l in result.stderr.splitlines() if l.strip()), "")
    if result.returncode == 124:
        return "timeout", ""
    if result.returncode != 0:
        return "crashed", first_err[:160]
    if check and lines(result.stdout) != lines(check.read_text(errors="replace")):
        return "wrong output", ""
    return "passed", ""


def lines(text):
    return text.rstrip("\n").splitlines()


def native_one(name, sources, check):
    binary = WORK / "bin" / name
    if not binary.exists():
        cmd = ["scala-cli", "--power", "package", "--quiet", "--native", "--native-version", "0.5.12",
               "--scala", "3.9.0", "--workspace", str(WORK / "ws" / name), "-f", "-o", str(binary),
               "--main-class", "Test"] + [str(s) for s in sources]
        if subprocess.run(cmd, capture_output=True).returncode != 0:
            return "link failed", ""
    return run_one(name, check, [str(binary)])


def native(filter_):
    (WORK / "bin").mkdir(parents=True, exist_ok=True)
    failing = {n for n, o, _ in read_results() if o not in ("passed", "compile failed", "skipped")}
    todo = [(n, s, c) for n, s, c, _ in tests(filter_) if n in failing]
    with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
        outcomes = list(pool.map(lambda t: native_one(*t), todo))
    rows = sorted((n, *o) for (n, _, _), o in zip(todo, outcomes))
    (WORK / "native.tsv").write_text("".join(f"{n}\t{o}\t{d}\n" for n, o, d in rows))
    print(f"{len(rows)} tests: " + ", ".join(f"{o} {sum(r[1] == o for r in rows)}" for o in sorted({r[1] for r in rows})))


def read_results(file="results.tsv"):
    path = WORK / file
    lines = path.read_text().splitlines() if path.exists() else []
    return [tuple((line.split("\t") + ["", ""])[:3]) for line in lines]


def build(filter_):
    (WORK / "logs").mkdir(parents=True, exist_ok=True)
    todo = [(n, s) for n, s, _, reason in tests(filter_) if reason is None]
    counts = {}
    with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
        for i, (name, outcome) in enumerate(pool.map(lambda t: compile_one(*t), todo), 1):
            counts[outcome] = counts.get(outcome, 0) + 1
            if i % 100 == 0:
                print(f"{i}/{len(todo)} {counts}", flush=True)
    print(f"{len(todo)} tests: {counts}")


def report(filter_):
    (WORK / "logs").mkdir(parents=True, exist_ok=True)
    rows = []
    runnable = []
    for name, _, check, reason in tests(filter_):
        if reason:
            rows.append((name, "skipped", reason))
        elif not (WORK / "nir" / name / ".done").exists():
            rows.append((name, "compile failed", ""))
        else:
            runnable.append((name, check))
    with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
        runs = pool.map(lambda t: run_one(*t), runnable)
        rows += [(name, *outcome) for (name, _), outcome in zip(runnable, runs)]
    in_native = {n: o for n, o, _ in read_results("native.tsv")}
    rows = sorted(
        (n, "fails in Scala Native", f"{in_native[n]}; {d}") if in_native.get(n, "passed") != "passed" and o != "passed"
        else (n, o, d)
        for n, o, d in rows
    )
    (WORK / "results.tsv").write_text("".join(f"{n}\t{o}\t{d}\n" for n, o, d in rows))
    outcomes = ["passed", "wrong output", "crashed", "timeout", "fails in Scala Native", "compile failed", "skipped"]
    counted = {o: sum(1 for r in rows if r[1] == o) for o in outcomes}
    ran = sum(counted[o] for o in outcomes[:4])
    table = "| Outcome | Tests |\n|---|---:|\n" + "".join(f"| {o} | {counted[o]} |\n" for o in outcomes)
    table += f"| **total** | **{len(rows)}** |\n\nPassed {counted['passed']} of {ran} run.\n"
    (WORK / "report.md").write_text(table)
    print(table)


if __name__ == "__main__":
    commands = {"build": build, "report": report, "native": native}
    if len(sys.argv) < 2 or sys.argv[1] not in commands:
        sys.exit(__doc__)
    commands[sys.argv[1]](sys.argv[2] if len(sys.argv) > 2 else "")
