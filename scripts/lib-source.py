#!/usr/bin/env python3
"""A Scala Native library from Maven compiled from its `-sources.jar`s through the own frontend.

Usage: scripts/lib-source.py LIBRARY

LIBRARY is one of LIBRARIES, a list of modules and the test programs run on them. For each
MODULE, Coursier fetches its published jar, `-sources.jar` and the jars it depends on (outside
org.scala-lang and org.scala-native); each `.scala` file of the sources is compiled on its own by
`scivolo-aot -d` against the TASTy of those jars into tmp/srclib/LIBRARY/MODULE/out/FILE/. Each
NIR file written is compared, as lib-compile.py compares them, with two oracles: the published
jar's (`published`) and the sources compiled by scala-cli with Scala 3.9.0 and Scala Native 0.5.12
against the published jar, for the sources it lacks (`scala-cli`). A NIR file that differs from
`scala-cli` is compared again with its source compiled alone by scala-cli (`scala-cli-file`).
MODULE/MODULE.jar is the
published jar with our NIR in place of its own; each test program tests/aot/TEST, its `//> using dep` lines blanked, is built against those jars and
the other dependencies, and its output compared with its `.check`.

Writes under tmp/srclib/LIBRARY/MODULE: results.tsv (file, ok|refused|timeout|crashed, reason, NIR
files written), cmp.tsv (oracle, NIR file, same|var-order|differ|missing, differing lines, 1 if var
slots were aligned), diffs/ORACLE/; under tmp/srclib/LIBRARY/tests/TEST: TEST.out; and prints the
summary lines.
"""
import collections
import importlib
import os
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

lib = importlib.import_module("lib-compile")

ROOT = lib.ROOT
AOT = ROOT / "target/release/scivolo-aot"
LIBRARIES = {
    "sourcecode": (["com.lihaoyi:sourcecode_native0.5_3:0.4.4"], ["sourcecode"]),
    "geny": (["com.lihaoyi:geny_native0.5_3:1.1.1"], []),
    "ujson": ([f"com.lihaoyi:{m}_native0.5_3:4.4.3" for m in ("upickle-core", "ujson", "upack")], ["ujson"]),
    "fansi": (["com.lihaoyi:fansi_native0.5_3:0.5.1"], []),
    "pprint": (["com.lihaoyi:pprint_native0.5_3:0.9.6"], ["pprint", "Lh24pprint"]),
    "upickle": ([f"com.lihaoyi:{m}_native0.5_3:4.4.3" for m in ("upickle-implicits", "upickle")], ["Lh24upickle"]),
    "mainargs": (["com.lihaoyi:mainargs_native0.5_3:0.7.8"], ["mainargs"]),
    "fastparse": (["com.lihaoyi:fastparse_native0.5_3:3.1.1"], ["fastparse", "fastparsews"]),
    "os-lib": (["com.lihaoyi:os-lib_native0.5_3:0.11.9-M8"], ["oslib", "Lh24oslib"]),
}
SCALA, NATIVE = "3.9.0", "0.5.12"
TIMEOUT = 120


def coursier(*args):
    if not shutil.which("coursier"):
        sys.exit("lib-source: coursier not found on PATH")
    r = subprocess.run(["coursier", "fetch", *args], stdout=subprocess.PIPE, text=True)
    if r.returncode != 0:
        sys.exit(f"lib-source: coursier fetch {' '.join(args)} failed")
    return [Path(j) for j in r.stdout.split()]


def fetch(coordinate):
    """The published jar and `-sources.jar` of `coordinate`, and the jars it depends on."""
    jars = coursier("--sources", "--default=true", "--intransitive", coordinate)
    sources = next(j for j in jars if j.name.endswith("-sources.jar"))
    published = next(j for j in jars if j != sources)
    deps = coursier("--exclude", "org.scala-lang:*", "--exclude", "org.scala-native:*", coordinate)
    return published, sources, [j for j in deps if j != published]


def scala_cli(src, root, out, deps):
    """The NIR directory `out` of `src` (a file or a directory under `root`) compiled by scala-cli, or None
    with its error printed."""
    jars = [a for j in deps for a in ("--extra-jars", str(j))]
    r = subprocess.run(["scala-cli", "--power", "compile", "--quiet", "--server=false", "--native", "--native-version",
                        NATIVE, "--scala", SCALA, "--scalac-option", "-sourceroot", "--scalac-option", str(root),
                        *jars, "--workspace", str(out.with_name(out.name + "-work")), "-d", str(out), str(src)],
                       capture_output=True, text=True, errors="replace")
    if r.returncode != 0:
        print(f"  scala-cli failed on {src.relative_to(root.parent)}: {r.stderr.strip()[-300:]}")
        return None
    return out


def classpath(jars):
    return [a for j in jars for a in ("--cp", str(j))]


def compile_files(work, jars, src):
    """The rows of results.tsv and the NIR directory of each file compiled."""
    results, outs = [], []
    for f in sorted(src.rglob("*.scala")):
        rel = str(f.relative_to(src))
        out = work / "out" / rel.removesuffix(".scala")
        state, reason, _ = lib.run([str(AOT), *classpath(jars), "--sourceroot", str(src), "-d", str(out), str(f)],
                                   TIMEOUT)
        results.append([rel, state, reason, sum(1 for _ in out.rglob("*.nir")) if state == "ok" else 0])
        if state == "ok":
            outs.append(out)
    return results, outs


def compare_with(name, oracle, out, paths, work):
    """The rows of cmp.tsv for the NIR files `paths` under `out` against NIR directory `oracle`."""
    rows = []
    for path in paths:
        state, lines, reordered = lib.compare(out / path, oracle / path, work / "diffs" / name / f"{path}.diff")
        rows.append([name, path, state, lines, int(reordered)])
    return rows


def summary(name, rows):
    counts = collections.Counter(r[2] for r in rows)
    return (f"  vs {name}: nir {counts['same']} same, {counts['differ']} differ, {counts['var-order']} var-order, "
            f"{counts['quote-pos']} quote-pos, {counts['missing']} missing")


def nir_files(out):
    return [str(p.relative_to(out)) for p in sorted(out.rglob("*.nir"))]


def compare_per_file(src, outs, joint, work, jars):
    """The rows of cmp.tsv against `scala-cli-file`, each source compiled alone by scala-cli, for the NIR
    files in `joint` (the rows against `scala-cli`) that differ."""
    differ = {r[1] for r in joint if r[2] == "differ"}
    rows = []
    for out in outs:
        paths = [p for p in nir_files(out) if p in differ]
        rel = out.relative_to(work / "out")
        if paths and (nir := scala_cli(src / f"{rel}.scala", src, work / "scala-cli-file" / rel, jars)):
            rows += compare_with("scala-cli-file", nir, out, paths, work)
    return rows


def write_jar(path, published, outs):
    """`published` with the NIR files under `outs` in place of its own."""
    ours = {str(p.relative_to(o)): p.read_bytes() for o in outs for p in o.rglob("*.nir")}
    with zipfile.ZipFile(published) as src, zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as dst:
        for info in src.infolist():
            dst.writestr(info, ours.pop(info.filename, None) or src.read(info.filename))
        for name, data in sorted(ours.items()):
            dst.writestr(name, data)


def run_test(name, work, jars):
    """`passed` if tests/aot/NAME, built against `jars`, prints its `.check`, else why not."""
    test, copy = ROOT / "tests/aot" / name, work / "src"
    copy.mkdir(parents=True)
    for f in test.glob("*.scala"):
        lines = f.read_text().splitlines(keepends=True)
        (copy / f.name).write_text("".join("\n" if l.startswith("//> using dep") else l for l in lines))
    exe, env = work / name, os.environ | {"SCIVOLO_CACHE": str(work / "cache")}
    state, reason, _ = lib.run([str(AOT), *classpath(jars), "-o", str(exe), str(copy)], TIMEOUT, env)
    if state != "ok":
        return f"compile {state}: {reason}"
    try:
        r = subprocess.run([str(exe)], capture_output=True, text=True, errors="replace", timeout=TIMEOUT, cwd=work)
    except subprocess.TimeoutExpired:
        return "timeout"
    (work / f"{name}.out").write_text(r.stdout + r.stderr)
    if r.returncode != 0:
        return f"exit code {r.returncode} ({name}.out)"
    return "passed" if r.stdout == (test / f"{name}.check").read_text() else f"output differs ({name}.out)"


def build_module(coordinate, work):
    """The published jar, our jar and the dependencies of module `coordinate`, after its summary lines."""
    published, sources, deps = fetch(coordinate)
    src, theirs = work / "src", work / "published"
    for jar, into in ((sources, src), (published, theirs)):
        with zipfile.ZipFile(jar) as z:
            z.extractall(into)
    results, outs = compile_files(work, [published, *deps], src)
    lib.write_tsv(work / "results.tsv", results)
    states = collections.Counter(r[1] for r in results)
    print(f"{work.name}: {states['ok']} ok, {states['refused']} refused, {states['timeout']} timeout, "
          f"{states['crashed']} crashed; "
          f"{sum(r[3] for r in results)} NIR files")
    for r in results:
        if r[1] != "ok":
            print(f"  {r[0]}: {r[1]} {r[2]}")
    oracles = {"published": theirs, "scala-cli": scala_cli(src, src, work / "scala-cli", [published, *deps])}
    cmp = []
    for oracle, nir in oracles.items():
        if nir:
            rows = [r for out in outs for r in compare_with(oracle, nir, out, nir_files(out), work)]
            cmp += rows
            print(f"{summary(oracle, rows)} (of {sum(1 for _ in nir.rglob('*.nir'))} in {nir.relative_to(ROOT)})")
    if oracles["scala-cli"]:
        rows = compare_per_file(src, outs, [r for r in cmp if r[0] == "scala-cli"], work, [published, *deps])
        cmp += rows
        matched = sum(r[2] in ("same", "var-order", "quote-pos") for r in rows)
        both = sum(r[0] == "scala-cli" and r[2] == "differ" for r in cmp) - matched
        print(f"{summary('scala-cli-file', rows)} (the differing files compiled alone); {both} differ from both")
    lib.write_tsv(work / "cmp.tsv", cmp)
    ours = work / f"{work.name}.jar"
    write_jar(ours, published, outs)
    return published, ours, deps


def main(name):
    work = ROOT / "tmp/srclib" / name
    shutil.rmtree(work, ignore_errors=True)
    modules, tests = LIBRARIES[name]
    built = [build_module(c, work / c.split(":")[1].removesuffix("_native0.5_3")) for c in modules]
    published = {p for p, _, _ in built}
    jars = [o for _, o, _ in built] + sorted({d for _, _, ds in built for d in ds} - published)
    for test in tests:
        print(f"  tests/aot/{test} on our {name}: {run_test(test, work / 'tests' / test, jars)}")


if __name__ == "__main__":
    if len(sys.argv) != 2 or sys.argv[1] not in LIBRARIES:
        sys.exit(__doc__)
    main(sys.argv[1])
