#!/usr/bin/env python3
"""Scala's library as a jar from jvm_lib_compile.py's class files and TASTy (ADR 0123).

Usage: scripts/jvm_lib_jar.py MODULE

MODULE is `scalalib`; needs a full `just jvm-lib-compile scalalib` run. Writes
tmp/lib-jvm/scalalib/libs/scala-library-3.9.0.jar beside links to the other jars of ref/libs (a
directory for SCIVOLO_LIBS), laid out as scala3's `scala-library-bootstrapped` jar:
- ours: the class files and TASTy of each compiled file; of a refused file, the oracle's (dotty);
- Scala 2.13: the published jar's classes of FILES_TO_COPY, their `TASTY` attribute set to our
  class's (`StripScala2Annotations.patchFile`), and its classes dotty does not generate;
- javac: the published jar's classes of the `.java` sources;
- resources: the published jar's other entries.
Stops unless the jar has the published jar's entries and each class's `TASTY` attribute is its
TASTy file's UUID. libs/jar.tsv lists each entry and its origin; prints the counts.
"""
import collections
import sys
import zipfile
from pathlib import Path

import jvm_check
import jvm_lib_oracle
from jvm_lib_compile import LIBS, ORACLE, WORK, read_tsv, write_tsv

PUBLISHED = LIBS / "scala-library-3.9.0.jar"
OUT = WORK / "libs"
MANIFEST = "META-INF/MANIFEST.MF"
# Beside the jar: each entry and its origin, which `dotty_jvm.py` counts in its report.
ORIGINS_TSV = "jar.tsv"
# scala3 3.9.0's project/ScalaLibraryFilesToCopy.scala: classes taken from Scala 2.13 with their
# nested classes (`NAME$…`).
FILES_TO_COPY = {
    "scala/Function0", "scala/Function1", "scala/Function2", "scala/Product1", "scala/Product2",
    "scala/Tuple1", "scala/Tuple2", "scala/collection/ArrayOps", "scala/collection/Stepper",
    "scala/collection/DoubleStepper", "scala/collection/IntStepper", "scala/collection/LongStepper",
    "scala/collection/immutable/DoubleVectorStepper", "scala/collection/immutable/IntVectorStepper",
    "scala/collection/immutable/LongVectorStepper", "scala/collection/immutable/Range",
    "scala/jdk/Accumulator", "scala/jdk/DoubleAccumulator", "scala/jdk/IntAccumulator",
    "scala/jdk/LongAccumulator",
    *(f"scala/jdk/FunctionWrappers$FromJava{n}" for n in (
        "DoubleBinaryOperator", "BooleanSupplier", "DoubleConsumer", "DoublePredicate", "DoubleSupplier",
        "DoubleToIntFunction", "DoubleToLongFunction", "IntBinaryOperator", "DoubleUnaryOperator",
        "IntPredicate", "IntConsumer", "IntSupplier", "IntToDoubleFunction", "IntToLongFunction",
        "IntUnaryOperator", "LongBinaryOperator", "LongConsumer", "LongPredicate", "LongSupplier",
        "LongToDoubleFunction", "LongToIntFunction", "LongUnaryOperator")),
    "scala/runtime/AbstractFunction0", "scala/runtime/AbstractFunction1", "scala/runtime/AbstractFunction2",
    "scala/runtime/AbstractPartialFunction", "scala/runtime/NonLocalReturnControl", "scala/util/Sorting",
    "scala/util/hashing/MurmurHash3",
}
ORIGINS = ["ours", "dotty (refused file)", "Scala 2.13", "javac", "resource"]


def copied(name):
    """Whether class file `name` is one scala3's build takes from Scala 2.13."""
    stem = name.removesuffix(".class")
    return any(stem == f or stem.startswith(f + "$") for f in FILES_TO_COPY)


def attribute(b, name):
    """The (offset, length) of class attribute `name` of class file bytes `b`, or None."""
    return next(((i, n) for a, i, n in jvm_check.class_attributes(b)[1] if a == name), None)


def source_file(b):
    pool, attributes = jvm_check.class_attributes(b)
    return next((pool[int.from_bytes(b[i:i + 2], "big")] for a, i, _ in attributes if a == "SourceFile"), "")


def patched(theirs, ours, name):
    """Scala 2.13's class file `theirs` with the `TASTY` attribute of our class file `ours`."""
    old, new = attribute(theirs, "TASTY"), ours and attribute(ours, "TASTY")
    if bool(old) != bool(new):
        sys.exit(f"{name}: a TASTY attribute in only one of Scala 2.13's class and ours")
    if not old:
        return theirs
    (i, n), (j, m) = old, new
    assert n == m == 16, name
    return theirs[:i] + ours[j:j + m] + theirs[i + n:]


def generated():
    """Our entries (path → (bytes, origin)): the outputs of each file, the oracle's of a refused one."""
    rows = read_tsv(WORK / "results.tsv")
    if not rows:
        sys.exit("no tmp/lib-jvm/scalalib/results.tsv: run `just jvm-lib-compile scalalib` first")
    oracle_by_file = jvm_lib_oracle.origins(ORACLE, {r[0] for r in rows})
    out = {}
    for file, state, *_ in rows:
        if state in ("same", "code differs", "attributes differ"):
            root = WORK / "out" / file.removesuffix(".scala")
            files, origin = [(p, root) for p in root.rglob("*") if p.suffix in (".class", ".tasty")], "ours"
        else:
            classes = [ORACLE / c for c in oracle_by_file[file]]
            files = [(p, ORACLE) for c in classes for p in (c, c.with_suffix(".tasty")) if p.exists()]
            origin = "dotty (refused file)"
        out |= {str(p.relative_to(r)): (p.read_bytes(), origin) for p, r in files}
    return out


def tasty_uuid(t):
    """The UUID of TASTy file bytes `t`: after the magic, three version numbers and the tooling string."""
    i = 4
    for field in range(4):
        n = 0
        while t[i] < 0x80:
            n, i = (n << 7) | t[i], i + 1
        n, i = (n << 7) | (t[i] & 0x7F), i + 1
        i += n if field == 3 else 0
    return t[i:i + 16]


def check(jar):
    """Stops unless `jar` has the published jar's entries and each class's `TASTY` attribute is
    the UUID of the TASTy file of its name (the class name without a trailing `$`)."""
    with zipfile.ZipFile(PUBLISHED) as z:
        published = {n for n in z.namelist() if not n.endswith("/")}
    if diff := sorted(published ^ (jar.keys() | {MANIFEST})):
        sys.exit(f"{len(diff)} entries in only one of the published jar and ours, e.g. {diff[:5]}")
    for name, (b, _) in jar.items():
        tasty = name.removesuffix(".class").removesuffix("$") + ".tasty"
        if name.endswith(".class") and (a := attribute(b, "TASTY")) and tasty in jar:
            if b[a[0]:a[0] + a[1]] != tasty_uuid(jar[tasty][0]):
                sys.exit(f"{name}: its TASTY attribute is not the UUID of {tasty}")


def entries():
    """The jar's entries in order: path → (bytes, origin)."""
    out = generated()
    oracle = {str(p.relative_to(ORACLE)) for p in ORACLE.rglob("*.class")}
    with zipfile.ZipFile(PUBLISHED) as z:
        for name in z.namelist():
            if name.endswith("/") or name.endswith(".tasty") or name == MANIFEST:
                continue
            b = z.read(name)
            if not name.endswith(".class"):
                out[name] = (b, "resource")
            elif source_file(b).endswith(".java"):
                out[name] = (b, "javac")
            elif copied(name) or name not in oracle:
                ours = out.get(name, (None,))[0]
                out[name] = (patched(b, ours, name), "Scala 2.13")
    return dict(sorted(out.items()))


def shares(jar):
    """The count of each origin of the entries of library `jar`, from the jar.tsv beside it."""
    tsv = jar.parent / ORIGINS_TSV
    if not tsv.exists():
        return f"origins unknown, no {ORIGINS_TSV} beside it"
    counts = collections.Counter(l.split("\t")[1] for l in tsv.read_text().splitlines())
    return f"{sum(counts.values())}: " + ", ".join(f"{counts[o]} {o}" for o in ORIGINS)


def write_jar(module):
    if module != "scalalib":
        sys.exit(__doc__)
    jar = entries()
    check(jar)
    OUT.mkdir(parents=True, exist_ok=True)
    for lib in LIBS.glob("*.jar"):
        if lib.name != PUBLISHED.name and not (OUT / lib.name).exists():
            (OUT / lib.name).symlink_to(lib.resolve())
    with zipfile.ZipFile(OUT / PUBLISHED.name, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr(MANIFEST, "Manifest-Version: 1.0\r\nScala-Compiler-Version: 3.9.0\r\n\r\n")
        for name, (b, _) in jar.items():
            z.writestr(zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0)), b, zipfile.ZIP_DEFLATED)
    write_tsv(OUT / ORIGINS_TSV, [[name, origin] for name, (_, origin) in jar.items()])
    print(f"{OUT / PUBLISHED.name}: entries {shares(OUT / PUBLISHED.name)}")


if __name__ == "__main__":
    write_jar(sys.argv[1] if len(sys.argv) == 2 else "")
