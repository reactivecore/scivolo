#!/usr/bin/env python3
"""Scala's library compiled for the JVM file by file through scivolo-jvm (ADR 0110).

Usage: scripts/jvm_lib_compile.py MODULE [FILTER]

MODULE is `scalalib`: the `.scala` files of scala-library's sources jar, unpatched, in
tmp/lib-jvm/scalalib/src (those whose paths contain FILTER). Each is compiled on its own with the
options of scalalib_sources.py into tmp/lib-jvm/scalalib/out/FILE/; each class file written is
compared with the published jar's through jvm_check.py's `code` (javap -c) and `attributes`
(javap -v) apart. A class of the jar belongs to the file its `SourceFile` attribute names.

Writes under tmp/lib-jvm/scalalib: results.tsv (file, same|code differs|attributes differ|refused|
crashed|timeout, reason, classes same, code differs, attributes differ, missing from the jar, only
in the jar), cmp.tsv (file, class, one of those states, first differing line), diffs/ (the jar's
class against ours), refusals.txt and code-groups.txt (by count), oracle-only.txt (the jar's
classes of no Scala file: Java sources, specialized classes), and prints a summary. With FILTER,
the rows of the files run replace those of the last run. Files worse than in baseline.tsv are
listed.
"""
import collections
import concurrent.futures
import difflib
import os
import re
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

import jvm_check
import scalalib_sources

ROOT = Path(__file__).resolve().parent.parent
LIBS = Path(os.environ.get("SCIVOLO_LIBS", ROOT / "ref/libs"))
WORK = ROOT / "tmp/lib-jvm/scalalib"
SRC, ORACLE = WORK / "src", WORK / "oracle"
JVM = ROOT / "target/release/scivolo-jvm"
OPTIONS = ["--scalalib", *(o for o in scalalib_sources.OPTIONS if o.startswith("--scalac-option="))]
STATES = ["same", "attributes differ", "code differs", "refused", "crashed", "timeout"]
SPECIALIZED = re.compile(r"\$mc[A-Z]+\$sp")
JOBS = os.cpu_count() or 4
TIMEOUT = 60


def unpacked(jar, out, names):
    """Directory `out` holding the entries of `jar` that `names` keeps, unpacked once."""
    if not (out / ".done").exists():
        shutil.rmtree(out, ignore_errors=True)
        with zipfile.ZipFile(jar) as z:
            z.extractall(out, [n for n in z.namelist() if names(n)])
        (out / ".done").touch()
    return out


def sources():
    keep = lambda n: n.endswith(".scala") and n not in scalalib_sources.IGNORED
    return unpacked(LIBS / "scala-library-3.9.0-sources.jar", SRC, keep)


def oracle():
    return unpacked(LIBS / scalalib_sources.LIBRARY, ORACLE, lambda n: n.endswith(".class"))


def origins(theirs, files):
    """The classes of `theirs` by the source in `files` their `SourceFile` attribute names in their
    package directory; under None the specialized ones (`$mcII$sp`) and those of no such source."""
    out = collections.defaultdict(set)
    for p in sorted(theirs.rglob("*.class")):
        b = p.read_bytes()
        pool, attributes = jvm_check.class_attributes(b)
        source = next((pool[int.from_bytes(b[i:i + 2], "big")] for n, i, _ in attributes if n == "SourceFile"), "")
        name = p.relative_to(theirs)
        file = str(name.parent / source)
        out[file if file in files and not SPECIALIZED.search(p.name) else None].add(str(name))
    return out


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
        return "missing", ""
    for state, view in (("code differs", jvm_check.code), ("attributes differ", jvm_check.attributes)):
        diff = list(difflib.unified_diff(view(theirs), view(ours), f"scalac/{name}", f"ours/{name}", lineterm=""))
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
    cmp += [[file, name, "only in the jar", ""] for name in sorted(expected - {c[1] for c in cmp})]
    counts = collections.Counter(c[2] for c in cmp)
    keys = ("same", "code differs", "attributes differ", "missing", "only in the jar")
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
    src, theirs = sources(), oracle()
    files = [str(f.relative_to(src)) for f in sorted(src.rglob("*.scala"))]
    by_file = origins(theirs, set(files))
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
          f"{classes['attributes differ']} attributes differ, {classes['missing']} not in the jar, "
          f"{classes['only in the jar']} only in the jar; {len(by_file[None])} oracle-only")
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
