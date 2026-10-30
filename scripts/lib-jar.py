#!/usr/bin/env python3
"""A module of Scala Native's library built as the jar Scala Native ships (research 41 §4).

Usage: scripts/lib-jar.py MODULE

Lowers each of the module's Scala 3 source directories as one unit, as lib-compile.py does each
file, and writes tmp/lib/MODULE/MODULE_native0.5_3-0.5.12.jar: that NIR, the resources under
src/main/resources/scala-native verbatim, the published jar's TASTy and a manifest as sbt writes
it. Compares the jar's NIR with lib-compile's oracle (diffs under tmp/lib/MODULE/jar-diffs) and
its other entries with the published jar's, and prints one summary line.
"""
import importlib
import shutil
import subprocess
import sys
import zipfile

lib = importlib.import_module("lib-compile")

VERSION = "0.5.12"
MANIFEST = """Manifest-Version: 1.0
Specification-Title: {m}
Specification-Version: {v}
Specification-Vendor: org.scala-native
Implementation-Title: {m}
Implementation-Version: {v}
Implementation-Vendor: org.scala-native
Implementation-Vendor-Id: org.scala-native
Implementation-URL: http://www.scala-native.org

"""


def lower(module, stage):
    """Lowers the module's source directories into `stage`; exits with the frontend's error."""
    for name in lib.SOURCE_DIRS:
        root = lib.SN / module / "src/main" / name
        files = [str(f) for f in sorted(root.rglob("*.scala"))]
        if not files:
            continue
        out = stage.parent / f"nir-{name}"
        cmd = [str(lib.LOWER), "--macros", *lib.OPTIONS.get(module, []), "--sourceroot", str(root), *files,
               str(out)]
        r = subprocess.run(cmd, capture_output=True, text=True, errors="replace", env=lib.ENV)
        if r.returncode != 0:
            sys.exit(f"{module}/{name}: {r.stderr.strip()[:500]}")
        shutil.copytree(out, stage, dirs_exist_ok=True)


def entries(module, stage, published):
    """The jar's entries in order: (name, bytes)."""
    found = {"META-INF/MANIFEST.MF": MANIFEST.format(m=module, v=VERSION).replace("\n", "\r\n").encode()}
    resources = lib.SN / module / "src/main/resources"
    for root in (stage, resources):
        if root.is_dir():
            found |= {str(p.relative_to(root)): p.read_bytes() for p in sorted(root.rglob("*")) if p.is_file()}
    found |= {n: published.read(n) for n in published.namelist() if n.endswith(".tasty")}
    return sorted(found.items(), key=lambda e: (e[0] != "META-INF/MANIFEST.MF", e[0]))


def write_jar(path, items):
    dirs = sorted({"/".join(n.split("/")[:i]) + "/" for n, _ in items for i in range(1, n.count("/") + 1)})
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as jar:
        for name, data in [(d, b"") for d in dirs if d != "META-INF/"] + items:
            jar.writestr(zipfile.ZipInfo(name, (2010, 1, 1, 0, 0, 0)), data, zipfile.ZIP_DEFLATED)


def main(module):
    work = lib.WORK / module / "jar"
    shutil.rmtree(work, ignore_errors=True)
    stage = work / "nir"
    stage.mkdir(parents=True)
    lower(module, stage)
    published = zipfile.ZipFile(lib.LIBS / f"{module}_native0.5_3-{VERSION}.jar")
    items = entries(module, stage, published)
    jar = lib.WORK / module / f"{module}_native0.5_3-{VERSION}.jar"
    write_jar(jar, items)
    theirs, counts, ours = lib.oracle(module), {"same": 0, "differ": 0, "missing": 0}, set()
    diffs = lib.WORK / module / "jar-diffs"
    shutil.rmtree(diffs, ignore_errors=True)
    for p in sorted(stage.rglob("*.nir")):
        rel = str(p.relative_to(stage))
        ours.add(rel)
        counts[lib.compare(p, theirs / rel, diffs / f"{rel}.diff")[0]] += 1
    only = sum(1 for p in theirs.rglob("*.nir") if str(p.relative_to(theirs)) not in ours)
    other = {n: d for n, d in items if not n.endswith((".nir", ".tasty"))}
    upstream = [n for n in published.namelist() if not n.endswith((".nir", ".tasty", ".class", "/"))]
    same = sum(1 for n in upstream if other.get(n) == published.read(n))
    print(f"{jar.relative_to(lib.ROOT)}: nir {counts['same']} same, {counts['differ']} differ, "
          f"{counts['missing']} missing from the oracle, {only} only in the oracle "
          f"({theirs.relative_to(lib.ROOT)}); other entries {same} of {len(upstream)} as published")


if __name__ == "__main__":
    if len(sys.argv) != 2 or sys.argv[1] not in lib.MODULES:
        sys.exit(__doc__)
    main(sys.argv[1])
