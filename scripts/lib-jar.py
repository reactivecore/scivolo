#!/usr/bin/env python3
"""A module of Scala Native's library built as the jar Scala Native ships (research 41 §4).

Usage: scripts/lib-jar.py MODULE (one of lib-compile.py's MODULES)

Lowers each of the module's Scala 3 source directories as one unit, as lib-compile.py does each
file, and writes tmp/lib/MODULE/<the published jar's name>: that NIR, the resources under
src/main/resources/scala-native verbatim, the published jar's TASTy and a manifest as sbt writes
it. scalalib's files are lowered one by one, as lib-compile.py does; its jar takes the NIR of the
classes Scala Native's build takes from Scala 2.13 from the published jar, and that of the files
the frontend refuses from lib-compile's oracle (listed in tmp/lib/scalalib/jar-taken.txt).
Compares the jar's own NIR with lib-compile's oracle (diffs under tmp/lib/MODULE/jar-diffs) and
its other entries with the published jar's, and prints one summary line.
"""
import concurrent.futures
import importlib
import shutil
import subprocess
import sys
import zipfile

import scalalib_sources

lib = importlib.import_module("lib-compile")

MANIFEST = """Manifest-Version: 1.0
Specification-Title: {m}
Specification-Version: {v}
Specification-Vendor: org.scala-native
Implementation-Title: {m}
Implementation-Version: {v}
Implementation-Vendor: org.scala-native
Implementation-Vendor-Id: org.scala-native
Implementation-URL: http://www.scala-native.org
{main}
"""
SCALALIB_MAIN = "Main-Class: scala.util.Properties\n"


def lower(module, stage):
    """Lowers the module's source directories into `stage`; exits with the frontend's error."""
    for name in lib.SOURCE_DIRS:
        root = lib.SN / module / "src/main" / name
        files = sorted(root.rglob("*.scala"))
        if not files:
            continue
        out = stage.parent / f"nir-{name}"
        r = subprocess.run(lib.lower_cmd(module, root, files, out), capture_output=True, text=True, errors="replace",
                           env=lib.ENV)
        if r.returncode != 0:
            sys.exit(f"{module}/{name}: {r.stderr.strip()[:500]}")
        shutil.copytree(out, stage, dirs_exist_ok=True)


def lower_scalalib(stage):
    """Lowers each scalalib file on its own into `stage` but the classes of SPECIALIZED; returns
    the files refused and the NIR files written by more than one file."""
    units = lib.units("scalalib", "")
    env = lib.ENV | {"SCIVOLO_LIBS": str(scalalib_sources.libs(lib.ORACLE_JARS))}

    def one(unit):
        root, files = unit
        out = stage.parent / "out" / str(files[0].relative_to(root)).removesuffix(".scala")
        state, reason, stdout = lib.run(lib.lower_cmd("scalalib", root, files, out), lib.TIMEOUT * len(files), env)
        return files[0].relative_to(root), state, reason, out, stdout.split()

    refused, twice = [], []
    with concurrent.futures.ThreadPoolExecutor(lib.JOBS) as pool:
        for file, state, reason, out, names in pool.map(one, units):
            if state != "ok":
                refused.append(f"{file}: {state} {reason}")
            for path in (n.removesuffix(".nir").replace(".", "/") + ".nir" for n in names):
                if scalalib_sources.specialized(path):
                    continue
                if (stage / path).exists():
                    twice.append(path)
                    continue
                (stage / path).parent.mkdir(parents=True, exist_ok=True)
                shutil.copy(out / path, stage / path)
    return refused, twice


def entries(module, stage, published):
    """The jar's entries in order, (name, bytes), and the names of the NIR files not lowered here:
    scalalib's taken from the published jar or the oracle."""
    version = f"3.9.0+{lib.VERSION}" if module == "scalalib" else lib.VERSION
    main = SCALALIB_MAIN if module == "scalalib" else ""
    manifest = MANIFEST.format(m=module, v=version, main=main).replace("\n", "\r\n").encode()
    found = {"META-INF/MANIFEST.MF": manifest}
    resources = lib.SN / module / "src/main/resources"
    for root in (stage, resources):
        if root.is_dir():
            found |= {str(p.relative_to(root)): p.read_bytes() for p in sorted(root.rglob("*")) if p.is_file()}
    found |= {n: published.read(n) for n in published.namelist() if n.endswith(".tasty")}
    taken = []
    if module == "scalalib":
        oracle = lib.oracle(module)
        specialized = {n: published.read(n) for n in published.namelist() if scalalib_sources.specialized(n)}
        rest = {n: (oracle / n).read_bytes() for n in sorted(str(p.relative_to(oracle)) for p in oracle.rglob("*.nir"))
                if n not in found and not scalalib_sources.specialized(n)}
        taken = [*specialized, *rest]
        found |= specialized | rest
    return sorted(found.items(), key=lambda e: (e[0] != "META-INF/MANIFEST.MF", e[0])), taken


def write_jar(path, items):
    dirs = sorted({"/".join(n.split("/")[:i]) + "/" for n, _ in items for i in range(1, n.count("/") + 1)})
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as jar:
        for name, data in [(d, b"") for d in dirs if d != "META-INF/"] + items:
            jar.writestr(zipfile.ZipInfo(name, (2010, 1, 1, 0, 0, 0)), data, zipfile.ZIP_DEFLATED)


def main(module):
    work, jar = lib.WORK / module / "jar", lib.WORK / module / lib.jar_name(module)
    shutil.rmtree(work, ignore_errors=True)
    jar.unlink(missing_ok=True)
    stage = work / "nir"
    stage.mkdir(parents=True)
    refused, twice = [], []
    if module == "scalalib":
        refused, twice = lower_scalalib(stage)
    else:
        lower(module, stage)
    published = zipfile.ZipFile(lib.LIBS / lib.jar_name(module))
    items, taken = entries(module, stage, published)
    write_jar(jar, items)
    theirs, ours = lib.oracle(module), set()
    counts = {"same": 0, "differ": 0, "var-order": 0, "quote-pos": 0, "missing": 0}
    diffs = lib.WORK / module / "jar-diffs"
    shutil.rmtree(diffs, ignore_errors=True)
    for p in sorted(stage.rglob("*.nir")):
        rel = str(p.relative_to(stage))
        ours.add(rel)
        counts[lib.compare(p, theirs / rel, diffs / f"{rel}.diff")[0]] += 1
    only = sum(1 for p in theirs.rglob("*.nir") if (rel := str(p.relative_to(theirs))) not in ours
               and not (module == "scalalib" and scalalib_sources.specialized(rel)))
    other = {n: d for n, d in items if not n.endswith((".nir", ".tasty"))}
    upstream = [n for n in published.namelist() if not n.endswith((".nir", ".tasty", ".class", "/"))]
    same = sum(1 for n in upstream if other.get(n) == published.read(n))
    print(f"{jar.relative_to(lib.ROOT)}: nir {counts['same']} same, {counts['differ']} differ, "
          f"{counts['var-order']} var-order, {counts['quote-pos']} quote-pos, "
          f"{counts['missing']} missing from the oracle, {only} only in the oracle ({theirs.relative_to(lib.ROOT)}); "
          f"other entries {same} of {len(upstream)} as published")
    if module == "scalalib":
        specialized = sum(1 for n in taken if scalalib_sources.specialized(n))
        lines = [*(f"refused {r}\n" for r in sorted(refused)), *(f"twice {p}\n" for p in sorted(twice)),
                 *(f"taken {n}\n" for n in taken)]
        (lib.WORK / module / "jar-taken.txt").write_text("".join(lines))
        print(f"  {len(refused)} files refused, {len(twice)} NIR files written twice; taken: {specialized} NIR "
              f"files of Scala 2.13's classes from the published jar, {len(taken) - specialized} of refused files "
              f"from the oracle")


if __name__ == "__main__":
    if len(sys.argv) != 2 or sys.argv[1] not in lib.MODULES:
        sys.exit(__doc__)
    main(sys.argv[1])
