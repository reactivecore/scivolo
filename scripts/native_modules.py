"""The modules of Scala Native's library, their sources, units and options, the `scivolo-lower`
command lowering a unit and the oracle its NIR is compared with (research 36)."""
import os
import zipfile
from pathlib import Path

import scalalib_sources

ROOT = Path(__file__).resolve().parent.parent
SN = ROOT / "ref/scala-native"
LIBS = Path(os.environ.get("SCIVOLO_LIBS", ROOT / "ref/libs"))
WORK = ROOT / "tmp/lib"
ORACLE_JARS = WORK / "sn/jars"
ENV = os.environ | {"SCIVOLO_LIBS": str(ORACLE_JARS)} if ORACLE_JARS.is_dir() else None
LOWER = ROOT / "target/release/scivolo-lower"
MODULES = ["nativelib", "clib", "posixlib", "windowslib", "javalib", "auxlib", "scalalib",
           "test-interface-sbt-defs", "test-interface-common", "test-interface", "junit-runtime"]
# The modules without a jar of their own: the module whose jar holds their classes, as Scala
# Native's build adds their sources to it, and the directory of those classes in that jar.
PART_OF = {"test-interface-common": ("test-interface", "scala/scalanative/testinterface/common/")}
SOURCE_DIRS = ["scala", "scala-3", "scala-2.13+"]
VERSION = "0.5.12"
WINDOWSLIB = LIBS / f"windowslib_native0.5_3-{VERSION}.jar"
JDK_STUBS = ROOT / "libs/jdk-stubs.jar"


def jar_name(module):
    return f"{module}_native0.5_3-{'3.9.0%2B' if module == 'scalalib' else ''}{VERSION}.jar"


def tasty_of(*modules):
    """The options showing the sources the JDK and the TASTy of `modules`: their jars of
    `just lib-oracle` if there, else the published ones."""
    jars = ORACLE_JARS if ORACLE_JARS.is_dir() else LIBS
    return ["--jdk-tasty", str(JDK_STUBS), *(o for m in modules for o in ("--tasty", str(jars / jar_name(m))))]


# The options of a module's sources: the TASTy they see beyond the frontend's library
# (windowslib's and the test modules', which the library lacks, and the JDK classes javalib does
# not define), and javalib's own classes as Scala classes, the JDK stubs' as Java classes.
OPTIONS = {
    "nativelib": ["--jdk-tasty", str(JDK_STUBS)],
    "windowslib": ["--tasty", str(WINDOWSLIB)],
    "javalib": ["--javalib", "--tasty", str(WINDOWSLIB), "--jdk-tasty", str(JDK_STUBS)],
    "scalalib": scalalib_sources.OPTIONS,
    "test-interface-sbt-defs": tasty_of("test-interface-sbt-defs"),
    "test-interface-common": tasty_of("test-interface", "test-interface-sbt-defs", "windowslib"),
    "test-interface": tasty_of("test-interface", "test-interface-sbt-defs", "windowslib"),
    "junit-runtime": tasty_of("junit-runtime", "test-interface-sbt-defs"),
}
# The groups of files of a module lowered as one unit, by path prefix in `src/main`: files that
# refer to each other's classes.
UNITS = {
    "auxlib": [["scala/scala/collection/concurrent/"],
               ["scala/scala/runtime/BoxedUnit.scala", "scala/scala/scalanative/runtime/BoxedUnit.scala"]],
    "javalib": [["scala/scala/scalanative/nio/fs/windows/WindowsPath.scala",
                 "scala/scala/scalanative/nio/fs/windows/WindowsPathParser.scala"]],
    "nativelib": [["scala-3/scala/scalanative/unsafe/UnsafePackageCompat.scala",
                   "scala/scala/scalanative/unsafe/package.scala", "scala/scala/scalanative/unsafe/Ptr.scala"]],
}
# The option that makes scivolo-jvm and scivolo-lower write the output of a unit's accepted files.
LEAVE_OUT = "--leave-out-refused"
JOBS = os.cpu_count() or 4
TIMEOUT = 60


SCALALIB_SOURCEROOT = WORK / "scalalib/sourceroot"
SCALALIB_LINK = SCALALIB_SOURCEROOT / "scalalib/target/scalaSources/3.9.0"


def link_scalalib_root(root):
    """Makes SCALALIB_LINK, Scala Native's `scalalib/target/scalaSources/3.9.0` under
    SCALALIB_SOURCEROOT, a link to `root`."""
    SCALALIB_LINK.parent.mkdir(parents=True, exist_ok=True)
    if SCALALIB_LINK.is_symlink() and SCALALIB_LINK.resolve() == root.resolve():
        return
    SCALALIB_LINK.unlink(missing_ok=True)
    SCALALIB_LINK.symlink_to(root.resolve())


def lower_cmd(module, unit, out):
    """The `scivolo-lower` command lowering the files of `unit`, (source directory, file) pairs,
    into `out`, the checkout the source root as in Scala Native's build (scalalib's through
    SCALALIB_LINK), against the JDK 17 API as that build but javalib's own (ADR 0095)."""
    sourceroot, files = SN, [f for _, f in unit]
    if module == "scalalib":
        sourceroot = SCALALIB_SOURCEROOT
        unit = [(SCALALIB_LINK, SCALALIB_LINK / f.relative_to(r)) for r, f in unit]
        files = [f for _, f in unit]
    roots = ["--sourceroot", str(sourceroot)]
    for r in dict.fromkeys(r for r, _ in unit):
        roots += ["--position-root", str(r)]
    jdk = [] if module == "javalib" else ["--jdk-17"]
    return [str(LOWER), LEAVE_OUT, "--macros", *OPTIONS.get(module, []), *jdk, *roots, *map(str, files), str(out)]


def oracle(module):
    """The NIR the module's is compared with: Scala Native's build of it from `just lib-oracle` if
    there, else the published jar's files, unpacked once; for a module of PART_OF, those of the
    module holding its classes."""
    module = PART_OF[module][0] if module in PART_OF else module
    if (WORK / "sn" / module).is_dir():
        return WORK / "sn" / module
    out = WORK / "jars" / module
    if not (out / ".done").exists():
        with zipfile.ZipFile(LIBS / jar_name(module)) as jar:
            jar.extractall(out)
        (out / ".done").touch()
    return out


def parts(module):
    """The modules of PART_OF whose classes `module`'s jar holds."""
    return [m for m, (whole, _) in PART_OF.items() if whole == module]


def from_own_sources(module, path):
    """Whether oracle NIR file `path` comes from `module`'s own sources, not from those of a
    module sharing its jar, or of Scala 2.13's classes in scalalib."""
    if module == "scalalib":
        return not scalalib_sources.specialized(path)
    if module in PART_OF:
        return path.startswith(PART_OF[module][1])
    return not any(path.startswith(PART_OF[m][1]) for m in parts(module))


def units(module, filter_):
    """The (source directory, file) pairs of each unit of `module`'s Scala sources with a file
    whose path contains `filter_`: each group of UNITS, each other file on its own."""
    found = {}
    roots = [scalalib_sources.sources(LIBS)] if module == "scalalib" else [SN / module / "src/main" / n for n in SOURCE_DIRS]
    for root in roots:
        for f in sorted(root.rglob("*.scala")):
            rel = f"{root.name}/{f.relative_to(root)}"
            group = next((i for i, g in enumerate(UNITS.get(module, [])) if rel.startswith(tuple(g))), rel)
            found.setdefault(group, []).append((root, f))
    return [u for u in found.values() if any(filter_ in str(f.relative_to(ROOT)) for _, f in u)]
