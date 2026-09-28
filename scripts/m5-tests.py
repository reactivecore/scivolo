#!/usr/bin/env python3
"""Milestone M5, scala/scala's test/junit in the VM (docs/conformance.md).

Usage: scripts/m5-tests.py build|report [FILTER]
"""
import collections
import concurrent.futures
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

SCALA, NATIVE, SCALA2 = "3.9.0", "0.5.12", "2.13.18"
ROOT = Path(__file__).resolve().parent.parent
JUNIT = ROOT / "ref/scala2/test/junit"
DENYLIST = ROOT / f"ref/scala-native/scala-partest-junit-tests/src/test/resources/{SCALA2}/DenylistedTests.txt"
HELPERS = [ROOT / "ref/scala2/src/testkit/scala/tools/testkit/AssertUtil.scala", ROOT / "tests/junit/runner.scala"]
WORK = ROOT / "tmp/m5" / f"{SCALA}-{NATIVE}"
LIBS = ROOT / "ref/libs"
SCALAI = ROOT / "target/release/scalai"
JOBS = max(2, (os.cpu_count() or 4) // 2)
TIMEOUT = int(os.environ.get("SCALAI_TIMEOUT", "900"))
# Helper source changes Scala 3 needs, by file name.
PATCHES = {"AssertUtil.scala": [("new Thread(group, () => test())", "new Thread(group, (() => test()): Runnable)")]}
ANSI = re.compile(r"\x1b\[[0-9;]*m")
ERROR_AT = re.compile(r"^\[error\] \./(?:.*?/)?src/(.+?\.scala):\d+:\d+$")
BOOTSTRAPPER = "$scalanative$junit$bootstrapper.nir"


def sources():
    """Test files relative to test/junit, except those Scala Native denylists."""
    if not JUNIT.is_dir():
        sys.exit(f"{JUNIT} missing: run `just m5-fetch`")
    deny = {l.strip() for l in DENYLIST.read_text().splitlines() if l.strip() and not l.startswith("#")}
    return sorted(str(p.relative_to(JUNIT)) for p in JUNIT.rglob("*.scala") if str(p.relative_to(JUNIT)) not in deny)


def compile_all(files):
    """The compiler output, or None when it succeeded, for `files` plus the helpers."""
    src, nir = WORK / "src", WORK / "nir"
    for d in (src, nir):
        shutil.rmtree(d, ignore_errors=True)
    for rel in files:
        (src / rel).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(JUNIT / rel, src / rel)
    for helper in HELPERS:
        text = helper.read_text()
        for old, new in PATCHES.get(helper.name, []):
            text = text.replace(old, new)
        (src / helper.name).write_text(text)
    cmd = ["scala-cli", "--power", "compile", "--native", "--native-version", NATIVE, "--scala", SCALA,
           "--dep", f"org.scala-native::junit-runtime::{NATIVE}",
           "--compiler-plugin", f"org.scala-native:::junit-plugin:{NATIVE}",
           "--scalac-option", "-Wconf:any:s", "--workspace", str(WORK / "ws"), "-d", str(nir), str(src)]
    r = subprocess.run(cmd, capture_output=True, text=True, cwd=ROOT)
    return None if r.returncode == 0 else ANSI.sub("", r.stdout + r.stderr)


def build(_filter):
    """Compiles all test files together, dropping those with errors until the rest compiles."""
    files, failed = sources(), {}
    while (output := compile_all([f for f in files if f not in failed])) is not None:
        lines = output.splitlines()
        errors = {m.group(1): lines[i + 1][8:] if i + 1 < len(lines) else ""
                  for i, l in enumerate(lines) if (m := ERROR_AT.match(l))}
        if not errors or any(Path(e).name in {h.name for h in HELPERS} for e in errors):
            (WORK / "compile.log").write_text(output)
            sys.exit(f"compilation failed outside the test files; see {WORK / 'compile.log'}")
        failed.update({e: m for e, m in errors.items() if e not in failed})
        print(f"dropping {len(errors)} files with errors", flush=True)
    rows = [(f, "compile failed", failed[f]) if f in failed else (f, "compiled", "") for f in files]
    (WORK / "compile.tsv").write_text("".join(f"{f}\t{o}\t{d}\n" for f, o, d in rows))
    print(f"{len(files)} files: {dict(collections.Counter(o for _, o, _ in rows))}")


def run_class(cls):
    """PASS/FAIL/SKIP lines of test class `cls`, or one line for a crash or timeout."""
    cmd = [str(SCALAI), "--cp", str(WORK / "nir"), "--cp", str(LIBS / f"junit-runtime_native0.5_3-{NATIVE}.jar"),
           "--cp", str(LIBS / f"test-interface-sbt-defs_native0.5_3-{NATIVE}.jar"), "runner", cls]
    env = dict(os.environ, SCALAI_TIMEOUT=str(TIMEOUT))
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, errors="replace", env=env, timeout=TIMEOUT + 10)
    except subprocess.TimeoutExpired:
        return [f"FAIL {cls}: timeout"]
    (WORK / "logs" / (cls + ".log")).write_text(r.stdout + "\n--- stderr ---\n" + r.stderr)
    lines = [l for l in r.stdout.splitlines() if re.match(rf"(PASS|FAIL|SKIP) {re.escape(cls)}[.: ]", l)]
    if not re.search(r"^passed \d+, failed \d+, skipped \d+$", r.stdout, re.M):
        err = next((l for l in r.stderr.splitlines() if l.strip()), f"exit {r.returncode}")
        lines.append(f"FAIL {cls}: run ended: {err[:160]}")
    return lines


def report(filter_):
    (WORK / "logs").mkdir(exist_ok=True)
    classes = sorted(str(p.relative_to(WORK / "nir"))[: -len(BOOTSTRAPPER)].replace("/", ".")
                     for p in (WORK / "nir").rglob("*" + BOOTSTRAPPER) if filter_ in p.name)
    (WORK / "results").mkdir(exist_ok=True)
    with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
        for cls, ls in zip(classes, pool.map(run_class, classes)):
            (WORK / "results" / (cls + ".txt")).write_text("".join(l + "\n" for l in ls))
    lines = [l for p in sorted((WORK / "results").iterdir()) for l in p.read_text().splitlines()]
    (WORK / "results.txt").write_text("".join(l + "\n" for l in lines))
    classes = sorted(p.stem for p in (WORK / "results").iterdir())
    counts = collections.Counter(l[:4] for l in lines)
    compile_rows = [l.split("\t") for l in (WORK / "compile.tsv").read_text().splitlines()]
    not_compiled = sum(1 for r in compile_rows if r[1] == "compile failed")
    table = ("| Outcome | Count |\n|---|---:|\n"
             f"| passed | {counts['PASS']} |\n| failed | {counts['FAIL']} |\n| skipped | {counts['SKIP']} |\n"
             f"| test classes | {len(classes)} |\n| files not compiling | {not_compiled} |\n")
    (WORK / "report.md").write_text(table)
    print(table)


if __name__ == "__main__":
    commands = {"build": build, "report": report}
    if len(sys.argv) < 2 or sys.argv[1] not in commands:
        sys.exit(__doc__)
    WORK.mkdir(parents=True, exist_ok=True)
    commands[sys.argv[1]](sys.argv[2] if len(sys.argv) > 2 else "")
