#!/usr/bin/env python3
"""Milestone M4, dotty's tests/run in the VM (docs/conformance.md).

Usage: scripts/dotty-tests.py build|report|native [FILTER]
"""
import collections
import concurrent.futures
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

from dotty_sources import tests

SCALA, NATIVE = "3.9.0", "0.5.12"
ROOT = Path(__file__).resolve().parent.parent
WORK = ROOT / "tmp/dotty" / f"{SCALA}-{NATIVE}"
SCALAI = ROOT / "target/release/scalai"
JOBS = max(2, (os.cpu_count() or 4) // 2)
TIMEOUT = int(os.environ.get("SCALAI_TIMEOUT", "60"))
FAILED = ("wrong output", "crashed", "timeout")
SAME = "fails in Scala Native too"
OUTCOMES = ["passed", *FAILED, SAME, "not built", "compile failed", "skipped"]


def scala_cli(command, name, sources, *extra):
    return ["scala-cli", "--power", command, "--quiet", "--native", "--native-version", NATIVE,
            "--scala", SCALA, "--workspace", str(WORK / "ws" / name), *extra, *map(str, sources)]


def lines(text):
    return text.rstrip("\n").splitlines()


def text(out):
    return out.decode(errors="replace") if isinstance(out, bytes) else out or ""


def run(cmd, check, log):
    """The outcome of `cmd` and a detail, with its output written to `log`."""
    env = dict(os.environ, SCALAI_TIMEOUT=str(TIMEOUT))
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, errors="replace", env=env, timeout=TIMEOUT + 10)
        out, err, code = r.stdout, r.stderr, r.returncode
    except subprocess.TimeoutExpired as e:
        out, err, code = text(e.stdout), text(e.stderr), None
    log.write_text(out + "\n--- stderr ---\n" + err)
    if code is None or "scalai: timed out" in err:
        return "timeout", ""
    if code != 0:
        return "crashed", next((l for l in err.splitlines() if l.strip()), "").replace("\t", " ")[:160]
    if check and lines(out) != lines(check.read_text(errors="replace")):
        return "wrong output", ""
    return "passed", ""


def stdout(name, suffix):
    """The stdout in a run log, with source positions as Scala Native prints them."""
    log = (WORK / "logs" / (name + suffix)).read_text(errors="replace").split("\n--- stderr ---\n")[0]
    log = re.sub(r"\([\w$.-]+\.scala:\d+\)", "(Unknown Source)", log)
    return [l for l in lines(log) if l != "<none>.main(Unknown Source)"]


def classify(name, outcome, detail, native):
    """A VM failure counts as Scala Native's when the native binary fails alike or does not link."""
    if outcome == "passed" or native is None:
        return outcome, detail
    n_outcome, n_detail = native
    if n_outcome == "link failed" or (n_outcome == outcome and stdout(name, ".run") == stdout(name, ".native")):
        return SAME, f"{n_outcome}: {n_detail or detail}"
    return outcome, f"{detail}; natively {n_outcome}"


def compile_one(name, groups):
    """Compiles the groups in order into one directory, each seeing the output before it."""
    out = WORK / "nir" / name
    if (out / ".done").exists():
        return "compiled"
    shutil.rmtree(out, ignore_errors=True)
    log = ""
    for i, sources in enumerate(groups):
        extra = ["--extra-jars", str(out)] if i else []
        r = subprocess.run(scala_cli("compile", name, sources, *extra, "-d", str(out)), capture_output=True, text=True)
        log += r.stdout + r.stderr
        if r.returncode != 0:
            break
    (WORK / "logs" / (name + ".compile")).write_text(log)
    if r.returncode != 0:
        return "compile failed"
    (out / ".done").touch()
    return "compiled"


def native_one(name, groups, check):
    binary = WORK / "bin" / name
    if not binary.exists():
        earlier = ["--extra-jars", str(WORK / "nir" / name)] if len(groups) > 1 else []
        extra = [*earlier, "-f", "-o", str(binary), "--main-class", "Test"]
        cmd = scala_cli("package", name, groups[-1], *extra)
        r = subprocess.run(cmd, capture_output=True, text=True, errors="replace")
        (WORK / "logs" / (name + ".link")).write_text(r.stdout + r.stderr)
        if r.returncode != 0:
            if "Unreachable symbols" not in r.stdout + r.stderr:
                return "link error", ""
            return "link failed", next((l for l in r.stdout.splitlines() if "Unknown " in l), "")[:160]
    return run([str(binary)], check, WORK / "logs" / (name + ".native"))


def load(file):
    path = WORK / file
    rows = [l.split("\t") + [""] for l in path.read_text().splitlines()] if path.exists() else []
    return {r[0]: (r[1], r[2]) for r in rows}


def save(file, rows):
    (WORK / file).write_text("".join(f"{n}\t{o}\t{d}\n" for n, (o, d) in sorted(rows.items())))


def parallel(fn, items):
    with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
        return list(pool.map(lambda t: fn(*t), items))


def build(filter_):
    todo = [(n, s) for n, s, _, reason in tests(filter_) if reason is None]
    counts = collections.Counter(parallel(compile_one, todo))
    print(f"{len(todo)} tests: {dict(counts)}")


def native(filter_):
    (WORK / "bin").mkdir(exist_ok=True)
    failing = {n for n, (o, _) in load("results.tsv").items() if o in (*FAILED, SAME)}
    todo = [(n, s, c) for n, s, c, _ in tests(filter_) if n in failing]
    rows = load("native.tsv") | dict(zip((n for n, _, _ in todo), parallel(native_one, todo)))
    save("native.tsv", rows)
    print(f"{len(todo)} tests: {dict(collections.Counter(rows[n][0] for n, _, _ in todo))}")


def report(filter_):
    rows, native_rows, runnable = load("results.tsv"), load("native.tsv"), []
    for name, _, check, reason in tests(filter_):
        if reason:
            rows[name] = ("skipped", reason)
        elif (WORK / "nir" / name / ".done").exists():
            runnable.append((name, check))
        elif (log := WORK / "logs" / (name + ".compile")).exists():
            plain = re.sub(r"\x1b\[[\d;]*m", "", log.read_text(errors="replace")).replace("\t", " ").splitlines()
            errors = [l.removeprefix("[error] ") for l in plain if l.startswith("[error] ") and ".scala:" not in l]
            rows[name] = ("compile failed", (errors or [next((l for l in plain if l.strip()), "")])[0][:160])
        else:
            rows[name] = ("not built", "")
    cmd = lambda name: [str(SCALAI), "--cp", str(WORK / "nir" / name), "Test"]
    runs = parallel(lambda n, c: run(cmd(n), c, WORK / "logs" / (n + ".run")), runnable)
    for (name, _), (outcome, detail) in zip(runnable, runs):
        rows[name] = classify(name, outcome, detail, native_rows.get(name))
    save("results.tsv", rows)
    counts = collections.Counter(o for o, _ in rows.values())
    table = "| Outcome | Tests |\n|---|---:|\n" + "".join(f"| {o} | {counts[o]} |\n" for o in OUTCOMES)
    table += f"| **total** | **{len(rows)}** |\n"
    (WORK / "report.md").write_text(table)
    print(table)


if __name__ == "__main__":
    commands = {"build": build, "report": report, "native": native}
    if len(sys.argv) < 2 or sys.argv[1] not in commands:
        sys.exit(__doc__)
    (WORK / "logs").mkdir(parents=True, exist_ok=True)
    commands[sys.argv[1]](sys.argv[2] if len(sys.argv) > 2 else "")
