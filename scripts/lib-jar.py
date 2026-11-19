#!/usr/bin/env python3
"""A module of Scala Native's library built as the jar Scala Native ships (research 41 §4).

Usage: scripts/lib-jar.py MODULE (one of lib-compile.py's MODULES but those of its PART_OF)

Lowers each of the module's Scala 3 source directories, with those of the modules of PART_OF it
holds, as one unit (all of a JOINT module's together),
as lib-compile.py does each file, and writes tmp/lib/MODULE/<the published jar's name>: that NIR, the resources under
src/main/resources/scala-native verbatim, the published jar's TASTy and a manifest as sbt writes
it. scalalib's files of scalalib_sources.py are lowered as one unit; its jar takes the NIR of the
classes Scala Native's build takes from Scala 2.13 from the published jar (listed in
tmp/lib/scalalib/jar-taken.txt). Compares the jar's own NIR with lib-compile's oracle (diffs under
tmp/lib/MODULE/jar-diffs) and its other entries with the published jar's, and prints one summary line.
"""
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
MAIN_CLASSES = {"scalalib": "scala.util.Properties", "test-interface": "scala.scalanative.testinterface.TestMain"}
# The modules whose source directories are lowered as one unit, as Scala Native's build does.
JOINT = {"nativelib", "test-interface-sbt-defs", "test-interface", "junit-runtime", "scalalib"}


def source_units(module):
    """The units of the module's sources, (name, (source directory, file) pairs): one per source
    directory of the module and of the modules it holds, all of them together for JOINT."""
    if module == "scalalib":
        units = [("all", [pair for unit in lib.units(module, "") for pair in unit])]
        lib.link_scalalib_root(units[0][1][0][0])
        return units
    units = []
    for part in (module, *lib.parts(module)):
        for name in lib.SOURCE_DIRS:
            root = lib.SN / part / "src/main" / name
            unit = [(root, f) for f in sorted(root.rglob("*.scala"))]
            if unit:
                units.append((f"{part}-{name}", unit))
    if module in JOINT:
        units = [("all", [pair for _, unit in units for pair in unit])]
    return units


def lower(module, stage):
    """Lowers the units of `source_units` into `stage`, scalalib's but the classes of SPECIALIZED;
    exits with the frontend's error."""
    env = lib.ENV
    if module == "scalalib":
        env = lib.ENV | {"SCIVOLO_LIBS": str(scalalib_sources.libs(lib.ORACLE_JARS))}
    for name, unit in source_units(module):
        out = stage.parent / f"nir-{name}"
        r = subprocess.run(lib.lower_cmd(module, unit, out), capture_output=True, text=True, errors="replace",
                           env=env)
        if r.returncode != 0:
            sys.exit(f"{module}/{name}: {r.stderr.strip()[:2000]}")
        shutil.copytree(out, stage, dirs_exist_ok=True)
    if module == "scalalib":
        for p in stage.rglob("*.nir"):
            if scalalib_sources.specialized(str(p.relative_to(stage))):
                p.unlink()


def entries(module, stage, published):
    """The jar's entries in order, (name, bytes), and the names of the NIR files not lowered here:
    scalalib's taken from the published jar."""
    version = f"3.9.0+{lib.VERSION}" if module == "scalalib" else lib.VERSION
    main = f"Main-Class: {MAIN_CLASSES[module]}\n" if module in MAIN_CLASSES else ""
    manifest = MANIFEST.format(m=module, v=version, main=main).replace("\n", "\r\n").encode()
    found = {"META-INF/MANIFEST.MF": manifest}
    resources = lib.SN / module / "src/main/resources"
    for root in (stage, resources):
        if root.is_dir():
            found |= {str(p.relative_to(root)): p.read_bytes() for p in sorted(root.rglob("*")) if p.is_file()}
    found |= {n: published.read(n) for n in published.namelist() if n.endswith(".tasty")}
    taken = {}
    if module == "scalalib":
        taken = {n: published.read(n) for n in published.namelist() if scalalib_sources.specialized(n)}
        found |= taken
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
    names = {n for n, _ in items}
    dropped = sum(1 for n in published.namelist() if n.endswith(".nir") and n not in names)
    print(f"{jar.relative_to(lib.ROOT)}: nir {counts['same']} same, {counts['differ']} differ, "
          f"{counts['var-order']} var-order, {counts['quote-pos']} quote-pos, "
          f"{counts['missing']} missing from the oracle, {only} only in the oracle ({theirs.relative_to(lib.ROOT)}), "
          f"{dropped} only in the published jar; other entries {same} of {len(upstream)} as published")
    if module == "scalalib":
        (lib.WORK / module / "jar-taken.txt").write_text("".join(f"taken {n}\n" for n in taken))
        print(f"  taken: {len(taken)} NIR files of Scala 2.13's classes from the published jar")


if __name__ == "__main__":
    if len(sys.argv) != 2 or sys.argv[1] not in lib.MODULES or sys.argv[1] in lib.PART_OF:
        sys.exit(__doc__)
    main(sys.argv[1])
