"""Scala Native's scalalib for Scala 3.9.0 as its build assembles it (research 36,
ref/scala-native/project/Settings.scala `commonScalalibSettings`): the `.scala` files of
scala-library's sources jar, patched with `git apply` and overridden by the files of
ref/scala-native-scalalib/scalalib/overrides-3, in tmp/lib/scalalib/src; beside it, the TASTy of
the JDK classes they use that javalib does not define, and the library jars they are lowered against."""
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OVERRIDES = ROOT / "ref/scala-native-scalalib/scalalib/overrides-3"
SRC = ROOT / "tmp/lib/scalalib/src"
JDK_ONLY = ROOT / "tmp/lib/scalalib/jdk-only.jar"
LIBS = ROOT / "tmp/lib/scalalib/libs"
LIBRARY = "scala-library-3.9.0.jar"
# auxlib's TASTy (the classes Scala's library defines in Java), which `just lib-oracle` builds.
AUXLIB = ROOT / "tmp/lib/sn/jars/auxlib_native0.5_3-0.5.12.jar"
OPTIONS = ["--scalalib", "--jdk-tasty", str(JDK_ONLY), "--jdk-tasty", str(AUXLIB),
           *(f"--scalac-option={o}" for o in ["-Yexplicit-nulls", "-language:implicitConversions", "-Wconf:any:silent"])]
# The files of scala-library's `library-aux` the build leaves out.
IGNORED = {f"scala/{n}.scala" for n in ["Any", "AnyRef", "Singleton", "Nothing", "Null", "AnyKind", "Matchable"]}
# The classes the published jar takes from the Scala 2.13 scalalib for their specializations
# (Build.scala's `overridenFiles`), with their companions and nested classes.
SPECIALIZED = [
    *(f"scala/{c}" for c in ["Function0", "Function1", "Function2", "Product1", "Product2", "Tuple1", "Tuple2"]),
    *(f"scala/collection/{c}" for c in ["ArrayOps", "Stepper", "DoubleStepper", "IntStepper", "LongStepper"]),
    *(f"scala/collection/immutable/{c}VectorStepper" for c in ["Double", "Int", "Long"]),
    "scala/collection/immutable/Range",
    *(f"scala/jdk/{c}Accumulator" for c in ["", "Double", "Int", "Long"]),
    *(f"scala/jdk/FunctionWrappers.FromJava{c}" for c in [
        "DoubleBinaryOperator", "BooleanSupplier", "DoubleConsumer", "DoublePredicate", "DoubleSupplier",
        "DoubleToIntFunction", "DoubleToLongFunction", "IntBinaryOperator", "DoubleUnaryOperator",
        "IntPredicate", "IntConsumer", "IntSupplier", "IntToDoubleFunction", "IntToLongFunction",
        "IntUnaryOperator", "LongBinaryOperator", "LongConsumer", "LongPredicate", "LongSupplier",
        "LongToDoubleFunction", "LongToIntFunction", "LongUnaryOperator"]),
    *(f"scala/runtime/{c}" for c in ["AbstractFunction0", "AbstractFunction1", "AbstractFunction2",
                                     "AbstractPartialFunction", "NonLocalReturnControl"]),
    "scala/util/Sorting", "scala/util/hashing/MurmurHash3",
]


def specialized(nir_path):
    """Whether NIR file `nir_path` (`scala/Function1.nir`) is one of SPECIALIZED's."""
    name = nir_path.removesuffix(".nir").replace("$", ".")
    return any(name == s or name.startswith(s + ".") for s in SPECIALIZED)


def sources(libs):
    """SRC, assembled once from the sources jar in `libs`, and JDK_ONLY, each time."""
    if not AUXLIB.exists():
        sys.exit(f"{AUXLIB.relative_to(ROOT)} missing (the published auxlib has no TASTy): run `just lib-oracle`")
    javalib = set(zipfile.ZipFile(ROOT / "libs/javalib-tasty-0.5.12.jar").namelist())
    JDK_ONLY.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(ROOT / "libs/jdk-stubs.jar") as stubs, zipfile.ZipFile(JDK_ONLY, "w") as out:
        for n in stubs.namelist():
            if n.endswith(".tasty") and n not in javalib:
                out.writestr(n, stubs.read(n))
    if (SRC / ".done").exists():
        return SRC
    shutil.rmtree(SRC, ignore_errors=True)
    SRC.mkdir(parents=True)
    with zipfile.ZipFile(libs / "scala-library-3.9.0-sources.jar") as jar:
        jar.extractall(SRC, [n for n in jar.namelist() if n.endswith(".scala") and n not in IGNORED])
    for f in sorted(OVERRIDES.rglob("*.scala")):
        shutil.copy(f, SRC / f.relative_to(OVERRIDES))
    for patch in sorted(OVERRIDES.rglob("*.scala.patch")):
        cmd = ["git", "apply", "--whitespace=fix", "--recount", str(patch)]
        r = subprocess.run(cmd, cwd=SRC, capture_output=True, text=True)
        if r.returncode != 0:
            sys.exit(f"{patch.relative_to(ROOT)} does not apply: {r.stderr.strip()}")
    (SRC / ".done").touch()
    return SRC


def libs(jars):
    """LIBS, the jars of directory `jars` with Scala's library's `LazyVals` TASTy replaced by
    that of SRC's patched source (whose `Waiting` declares `countDown` and `await`), compiled
    once by scala-cli: no published jar holds that TASTy."""
    lazyvals = LIBS / "lazyvals/scala/runtime/LazyVals.tasty"
    LIBS.mkdir(parents=True, exist_ok=True)
    if not lazyvals.exists():
        cmd = ["scala-cli", "--power", "compile", "--server=false", "--offline", "-S", "3.9.0", "-O", "-experimental",
               "--jar", str(jars / "nativelib_native0.5_3-0.5.12.jar"), "--compile-output", str(LIBS / "lazyvals"),
               str(SRC / "scala/runtime/LazyVals.scala")]
        r = subprocess.run(cmd, capture_output=True, text=True)
        if r.returncode != 0 or not lazyvals.exists():
            sys.exit(f"compiling LazyVals.scala failed: {r.stderr.strip()}")
    for jar in jars.glob("*.jar"):
        if not (LIBS / jar.name).exists() and jar.name != LIBRARY:
            (LIBS / jar.name).symlink_to(jar.resolve())
    if not (LIBS / LIBRARY).exists():
        part = LIBS / f"{LIBRARY}.part"
        with zipfile.ZipFile(jars / LIBRARY) as lib, zipfile.ZipFile(part, "w") as out:
            for n in lib.namelist():
                out.writestr(n, lazyvals.read_bytes() if n == "scala/runtime/LazyVals.tasty" else lib.read(n))
        part.rename(LIBS / LIBRARY)
    return LIBS
