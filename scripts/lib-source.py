#!/usr/bin/env python3
"""A Scala Native library from Maven compiled from its `-sources.jar` through the own frontend.

Usage: scripts/lib-source.py LIBRARY

LIBRARY is one of LIBRARIES. Coursier fetches the library's published jar and `-sources.jar`;
each `.scala` file of the sources is compiled on its own by `scivolo-aot -d` against the published
jar's TASTy into tmp/srclib/LIBRARY/out/FILE/. Each NIR file written is compared, as
lib-compile.py compares them, with two oracles: the published jar's (`published`) and the sources
compiled by scala-cli with Scala 3.9.0 and Scala Native 0.5.12 (`scala-cli`).
tmp/srclib/LIBRARY/LIBRARY.jar is the published jar with our NIR in place of its own; the program
tests/aot/LIBRARY, its `//> using dep` lines blanked, is built against it and its output compared
with its `.check`.

Writes under tmp/srclib/LIBRARY: results.tsv (file, ok|refused|timeout, reason, NIR files
written), cmp.tsv (oracle, NIR file, same|var-order|differ|missing, differing lines, 1 if var
slots were aligned), diffs/ORACLE/, test.out, and prints the summary lines.
"""
import collections
import os
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

from nir_compare import compare

ROOT = Path(__file__).resolve().parent.parent
AOT = ROOT / "target/release/scivolo-aot"
LIBRARIES = {"sourcecode": "com.lihaoyi:sourcecode_native0.5_3:0.4.4"}
SCALA, NATIVE = "3.9.0", "0.5.12"
TIMEOUT = 120


def fetch(coordinate):
    """The paths of the published jar and the `-sources.jar` of `coordinate`."""
    r = subprocess.run(["coursier", "fetch", "--sources", "--default=true", "--intransitive", coordinate],
                       capture_output=True, text=True, check=True)
    jars = r.stdout.split()
    sources = next(j for j in jars if j.endswith("-sources.jar"))
    return Path(next(j for j in jars if j != sources)), Path(sources)


def scala_cli(src, work):
    """The NIR directory of `src` compiled by scala-cli, or None with its error printed."""
    out = work / "scala-cli"
    r = subprocess.run(["scala-cli", "--power", "compile", "--quiet", "--native", "--native-version", NATIVE,
                        "--scala", SCALA, "--workspace", str(work / "scala-cli-work"), "-d", str(out), str(src)],
                       capture_output=True, text=True, errors="replace")
    if r.returncode != 0:
        print(f"  scala-cli failed: {r.stderr.strip()[-300:]}")
        return None
    return out


def aot(args, env=None):
    """`scivolo-aot args`: None if it succeeded, else the first line of its error."""
    try:
        r = subprocess.run([str(AOT), *args], capture_output=True, text=True, errors="replace", timeout=TIMEOUT,
                           env=env)
    except subprocess.TimeoutExpired:
        return "timeout"
    return None if r.returncode == 0 else next((l for l in r.stderr.splitlines() if l.strip()), "failed")[:300]


def compile_files(work, jar, src):
    """The rows of results.tsv and the NIR directory of each file compiled."""
    results, outs = [], []
    for f in sorted(src.rglob("*.scala")):
        rel = str(f.relative_to(src))
        out = work / "out" / rel.removesuffix(".scala")
        if error := aot(["--cp", str(jar), "-d", str(out), str(f)]):
            results.append([rel, "timeout" if error == "timeout" else "refused", error.replace("\t", " "), 0])
            continue
        results.append([rel, "ok", "", sum(1 for _ in out.rglob("*.nir"))])
        outs.append(out)
    return results, outs


def compare_with(name, oracle, outs, work):
    """The rows of cmp.tsv against NIR directory `oracle`, and the summary line."""
    rows, counts = [], collections.Counter()
    for out in outs:
        for p in sorted(out.rglob("*.nir")):
            path = str(p.relative_to(out))
            state, lines, reordered = compare(p, oracle / path, work / "diffs" / name / f"{path}.diff")
            counts[state] += 1
            rows.append([name, path, state, lines, int(reordered)])
    return rows, (f"  vs {name}: nir {counts['same']} same, {counts['differ']} differ, "
                  f"{counts['var-order']} var-order, {counts['missing']} missing")


def write_jar(path, published, outs):
    """`published` with the NIR files under `outs` in place of its own."""
    ours = {str(p.relative_to(o)): p.read_bytes() for o in outs for p in o.rglob("*.nir")}
    with zipfile.ZipFile(published) as src, zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as dst:
        for info in src.infolist():
            dst.writestr(info, ours.pop(info.filename, None) or src.read(info.filename))
        for name, data in sorted(ours.items()):
            dst.writestr(name, data)


def run_test(name, work, jar):
    """`passed` if tests/aot/NAME, built against `jar`, prints its `.check`, else why not."""
    test, copy = ROOT / "tests/aot" / name, work / "test"
    copy.mkdir()
    for f in test.glob("*.scala"):
        lines = f.read_text().splitlines(keepends=True)
        (copy / f.name).write_text("".join("\n" if l.startswith("//> using dep") else l for l in lines))
    exe, env = work / name, os.environ | {"SCIVOLO_CACHE": str(work / "cache")}
    if error := aot(["--cp", str(jar), "-o", str(exe), str(copy)], env=env):
        return f"compile: {error}"
    out = subprocess.run([str(exe)], capture_output=True, text=True, errors="replace", timeout=TIMEOUT).stdout
    (work / "test.out").write_text(out)
    return "passed" if out == (test / f"{name}.check").read_text() else "output differs (test.out)"


def write_tsv(path, rows):
    path.write_text("".join("\t".join(map(str, r)) + "\n" for r in rows))


def main(name):
    work = ROOT / "tmp/srclib" / name
    shutil.rmtree(work, ignore_errors=True)
    published, sources = fetch(LIBRARIES[name])
    src, theirs = work / "src", work / "published"
    for jar, into in ((sources, src), (published, theirs)):
        with zipfile.ZipFile(jar) as z:
            z.extractall(into)
    results, outs = compile_files(work, published, src)
    write_tsv(work / "results.tsv", results)
    states = collections.Counter(r[1] for r in results)
    print(f"{name}: {states['ok']} ok, {states['refused']} refused, {states['timeout']} timeout; "
          f"{sum(r[3] for r in results)} NIR files")
    for r in results:
        if r[1] != "ok":
            print(f"  {r[0]}: {r[1]} {r[2]}")
    oracles = {"published": theirs, "scala-cli": scala_cli(src, work)}
    cmp = []
    for oracle, nir in oracles.items():
        if nir:
            rows, line = compare_with(oracle, nir, outs, work)
            cmp += rows
            print(f"{line} (of {sum(1 for _ in nir.rglob('*.nir'))} in {nir.relative_to(ROOT)})")
    write_tsv(work / "cmp.tsv", cmp)
    jar = work / f"{name}.jar"
    write_jar(jar, published, outs)
    print(f"  tests/aot/{name} on {jar.relative_to(ROOT)}: {run_test(name, work, jar)}")


if __name__ == "__main__":
    if len(sys.argv) != 2 or sys.argv[1] not in LIBRARIES:
        sys.exit(__doc__)
    main(sys.argv[1])
