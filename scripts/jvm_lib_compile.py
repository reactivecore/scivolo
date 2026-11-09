#!/usr/bin/env python3
"""Scala's library compiled for the JVM file by file through scivolo-jvm (ADR 0110).

Usage: scripts/jvm_lib_compile.py MODULE [FILTER]

MODULE is `scalalib`: the `.scala` files of scala-library's sources jar, unpatched, in
tmp/lib-jvm/scalalib/sources (those whose paths contain FILTER). Each is compiled on its own with
LIBRARY_OPTIONS into tmp/lib-jvm/scalalib/out/FILE/; each class file written is compared through
jvm_check.py's `code` (javap -c) and `attributes` (javap -v) apart with the oracle's: the class
files dotty 3.9.0 compiles all `.scala` and `.java` files of the sources jar to in one run with
LIBRARY_OPTIONS, in tmp/lib-jvm/scalalib/dotty. An oracle class belongs to the file its
`SourceFile` attribute names in its package directory.

Writes under tmp/lib-jvm/scalalib: results.tsv (file, same|code differs|attributes differ|refused|
crashed|timeout, reason, classes same, code differs, attributes differ, only ours, only in the
oracle), cmp.tsv (file, class, one of those states, first differing line), diffs/ (the oracle's
class against ours), refusals.txt and code-groups.txt (by count), oracle-only.txt (the oracle's
classes of no Scala file), and prints a summary. With FILTER, the rows of the files run replace
those of the last run. Files worse than in baseline.tsv are listed.
"""
import collections
import concurrent.futures
import difflib
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

import jvm_check
import jvm_lib_oracle

ROOT = Path(__file__).resolve().parent.parent
LIBS = Path(os.environ.get("SCIVOLO_LIBS", ROOT / "ref/libs"))
WORK = ROOT / "tmp/lib-jvm/scalalib"
SRC, ORACLE = WORK / "sources", WORK / "dotty"
JVM = ROOT / "target/release/scivolo-jvm"
# The options scivolo-jvm does not take: it reads UTF-8 and writes Java 17 class files only.
FIXED = {"-encoding", "UTF8", "--java-output-version:17"}
OPTIONS = ["--scalalib", *(f"--scalac-option={o}" for o in jvm_lib_oracle.LIBRARY_OPTIONS if o not in FIXED)]
STATES = ["same", "attributes differ", "code differs", "refused", "crashed", "timeout"]
JOBS = os.cpu_count() or 4
TIMEOUT = 60


def run(file, out):
    """(state, reason) of compiling `file` (relative to SRC) into `out`."""
    shutil.rmtree(out, ignore_errors=True)
    cmd = [str(JVM), *OPTIONS, "-d", str(out), file]
    try:
        r = subprocess.run(cmd, cwd=SRC, capture_output=True, text=True, errors="replace", timeout=TIMEOUT)
    except subprocess.TimeoutExpired:
        return "timeout", ""
    if r.returncode == 0:
        return "ok", ""
    if m := re.search(r"panicked at (\S+):\n(.*)", r.stderr):
        return "crashed", f"{m[2]} at {m[1]}".replace("\t", " ")[:200]
    return "refused", next((l for l in r.stderr.splitlines() if l.strip()), "").replace("\t", " ")[:200]


def first_change(diff):
    """The first line `diff` removes or adds, numbers replaced by `N`."""
    line = next((l for l in diff[2:] if l[:1] in "-+"), "")
    return re.sub(r"\d+", "N", re.sub(r"\s+", " ", line.strip()))[:160]


def compare(name, ours, theirs):
    """(state, first differing line) of class file `name`; writes the diff of one that differs."""
    if not theirs.exists():
        return "only ours", ""
    for state, view in (("code differs", jvm_check.code), ("attributes differ", jvm_check.attributes)):
        diff = list(difflib.unified_diff(view(theirs), view(ours), f"dotty/{name}", f"ours/{name}", lineterm=""))
        if diff:
            path = WORK / "diffs" / f"{name}.diff"
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("\n".join(diff) + "\n")
            return state, first_change(diff)
    return "same", ""


def one(file, expected):
    """The row of results.tsv and the rows of cmp.tsv of source `file`, whose classes in the jar
    are `expected`; a timeout runs once more."""
    out = WORK / "out" / file.removesuffix(".scala")
    state, reason = run(file, out)
    if state == "timeout":
        state, reason = run(file, out)
    if state != "ok":
        return [file, state, reason, 0, 0, 0, 0, len(expected)], []
    cmp = []
    for p in sorted(out.rglob("*.class")):
        name = str(p.relative_to(out))
        cmp.append([file, name, *compare(name, p, ORACLE / name)])
    cmp += [[file, name, "only in the oracle", ""] for name in sorted(expected - {c[1] for c in cmp})]
    counts = collections.Counter(c[2] for c in cmp)
    keys = ("same", "code differs", "attributes differ", "only ours", "only in the oracle")
    worst = "attributes differ" if len(cmp) == counts["same"] + counts["attributes differ"] else "code differs"
    state = worst if len(cmp) > counts["same"] else "same"
    return [file, state, "", *(counts[k] for k in keys)], cmp


def read_tsv(path):
    return [l.split("\t") for l in path.read_text().splitlines()] if path.exists() else []


def write_tsv(path, rows):
    path.write_text("".join("\t".join(map(str, r)) + "\n" for r in rows))


def grouped(path, keys):
    """Writes the count of each of `keys` to `path`; its 10 largest lines."""
    lines = [f"{c:5} {k}" for k, c in collections.Counter(keys).most_common()]
    path.write_text("".join(l + "\n" for l in lines))
    return lines[:10]


def compile_module(filter_):
    keep = filter_ != ""
    if not keep:
        shutil.rmtree(WORK / "diffs", ignore_errors=True)
    WORK.mkdir(parents=True, exist_ok=True)
    src = jvm_lib_oracle.sources(LIBS / "scala-library-3.9.0-sources.jar", SRC)
    theirs = jvm_lib_oracle.oracle(src, ORACLE)
    files = [str(f.relative_to(src)) for f in sorted(src.rglob("*.scala"))]
    by_file = jvm_lib_oracle.origins(theirs, set(files))
    todo = [f for f in files if filter_ in f]
    with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
        done = list(pool.map(lambda f: one(f, by_file[f]), todo))
    rows = {r[0]: r for r in read_tsv(WORK / "results.tsv")} if keep else {}
    rows |= {r[0]: r for r, _ in done}
    write_tsv(WORK / "results.tsv", sorted(rows.values()))
    ran = {r[0] for r, _ in done}
    old_cmp = [c for c in read_tsv(WORK / "cmp.tsv") if c[0] not in ran] if keep else []
    compared = sorted(old_cmp + [c for _, cmp in done for c in cmp])
    write_tsv(WORK / "cmp.tsv", compared)
    failed = [r for r in rows.values() if r[1] in ("refused", "crashed", "timeout")]
    refusals = grouped(WORK / "refusals.txt", (f"{r[1]}: " + re.sub(r" at ([^ ]+:)?\d+:\d+", "", r[2]) for r in failed))
    codes = grouped(WORK / "code-groups.txt", (c[3] for c in compared if c[2] == "code differs"))
    states = collections.Counter(r[1] for r in rows.values())
    classes = collections.Counter(c[2] for c in compared)
    print(f"scalalib (JVM): {len(rows)} files: " + ", ".join(f"{states[s]} {s}" for s in STATES))
    print(f"  classes: {classes['same']} same, {classes['code differs']} code differ, "
          f"{classes['attributes differ']} attributes differ, {classes['only ours']} only ours, "
          f"{classes['only in the oracle']} only in the oracle; {len(by_file[None])} oracle-only")
    (WORK / "oracle-only.txt").write_text("".join(f"{p}\n" for p in sorted(by_file[None])))
    for title, lines in (("refusals", refusals), ("code differences, by first differing line", codes)):
        print(f"  top {title}:", *(f"    {l}" for l in lines), sep="\n")
    baseline = {r[0]: r for r in read_tsv(WORK / "baseline.tsv")}
    rank = {s: i for i, s in enumerate(STATES)}
    for r in sorted(rows.values()):
        old = baseline.get(r[0])
        if old and (rank[r[1]] > rank[old[1]] or int(r[3]) < int(old[3])):
            print(f"  worse: {r[0]}: {r[1]} {r[2]} (baseline: {old[1]}, {old[3]} same)")


if __name__ == "__main__":
    if len(sys.argv) not in (2, 3) or sys.argv[1] != "scalalib":
        sys.exit(__doc__)
    compile_module(sys.argv[2] if len(sys.argv) == 3 else "")
