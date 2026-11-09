"""The sources of scala-library 3.9.0 and the class files dotty 3.9.0 compiles all of them to in one
run with the library's options, the oracle of jvm_lib_compile.py (ADR 0110)."""
import collections
import shutil
import subprocess
import zipfile

import jvm_check
import scalalib_sources

DOTC = "org.scala-lang:scala3-compiler_3:3.9.0"
# The options of scala3 3.9.0's `scala-library-bootstrapped` (project/Build.scala) without
# `-opt -opt-inline:**,!java.**` and `-sourcepath`.
LIBRARY_OPTIONS = [
    "-feature", "-deprecation", "-unchecked", "-Werror",
    "-Wconf:msg=package scala contains object and package with same name:i",
    "-Wconf:msg=The RHS of reassignment must be transitively initialized:i",
    "-Wconf:msg=Could not verify that the method argument is transitively initialized:i",
    "-Wconf:msg=may cause initialization errors:i",
    "-Wconf:cat=deprecation&origin=scala\\.collection\\.Iterable\\.stringPrefix:s",
    "-encoding", "UTF8", "-language:implicitConversions", "--java-output-version:17", "-Yexplicit-nulls", "-Wsafe-init",
]


def sources(jar, out):
    """Directory `out` holding the `.scala` and `.java` files of sources jar `jar`, unpacked once."""
    if not (out / ".done").exists():
        shutil.rmtree(out, ignore_errors=True)
        with zipfile.ZipFile(jar) as z:
            names = [n for n in z.namelist() if n.endswith((".scala", ".java")) and n not in scalalib_sources.IGNORED]
            z.extractall(out, names)
        (out / ".done").touch()
    return out


def oracle(src, out):
    """Directory `out` holding the class files of the sources in `src`, compiled once."""
    if not (out / ".done").exists():
        cp = subprocess.run(["coursier", "fetch", "--classpath", DOTC], capture_output=True, text=True, check=True)
        files = sorted(str(p.relative_to(src)) for p in src.rglob("*") if p.suffix in (".scala", ".java"))
        shutil.rmtree(out, ignore_errors=True)
        out.mkdir(parents=True)
        main = ["java", "-Xss8m", "-cp", cp.stdout.strip(), "dotty.tools.dotc.Main", "-classpath", "", "-sourcepath", "."]
        subprocess.run([*main, "-d", str(out), *LIBRARY_OPTIONS, *files], cwd=src, check=True)
        (out / ".done").touch()
    return out


def origins(classes, files):
    """The class files under `classes` by the source in `files` their `SourceFile` attribute names
    in their package directory; under None those of no such source."""
    out = collections.defaultdict(set)
    for p in sorted(classes.rglob("*.class")):
        b = p.read_bytes()
        pool, attributes = jvm_check.class_attributes(b)
        source = next((pool[int.from_bytes(b[i:i + 2], "big")] for n, i, _ in attributes if n == "SourceFile"), "")
        name = p.relative_to(classes)
        file = str(name.parent / source)
        out[file if file in files else None].add(str(name))
    return out
