#!/usr/bin/env python3
"""The files of native/ that differ from the C resources of Scala Native's jars (ADR 0042).

Usage: scripts/native-stale.py

Each native/MODULE/scala-native/PATH is compared with entry scala-native/PATH of MODULE's jar
in SCIVOLO_LIBS (default ref/libs); the patched files and native/scivolo are left out. Prints
`differs PATH` or `not in the jar PATH` for each, and fails if any.
"""
import os
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NATIVE = ROOT / "native"
LIBS = Path(os.environ.get("SCIVOLO_LIBS", ROOT / "ref/libs"))
PATCHED = {"nativelib/scala-native/gc/immix/Marker.c", "nativelib/scala-native/gc/immix_commix/headers/ObjectHeader.h"}


def stale():
    found, checked = [], 0
    for module in sorted(d for d in NATIVE.iterdir() if (d / "scala-native").is_dir()):
        with zipfile.ZipFile(LIBS / f"{module.name}_native0.5_3-0.5.12.jar") as jar:
            entries = set(jar.namelist())
            for f in sorted(p for p in (module / "scala-native").rglob("*") if p.is_file()):
                rel = str(f.relative_to(NATIVE))
                entry = str(f.relative_to(module))
                if rel in PATCHED:
                    continue
                checked += 1
                if entry not in entries:
                    found.append(f"not in the jar {rel}")
                elif jar.read(entry) != f.read_bytes():
                    found.append(f"differs {rel}")
    if found:
        sys.exit("\n".join(found))
    print(f"ok    native: {checked} files as in the jars")


if __name__ == "__main__":
    if len(sys.argv) != 1:
        sys.exit(__doc__)
    stale()
