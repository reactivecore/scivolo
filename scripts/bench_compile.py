#!/usr/bin/env python3
"""Compile speed of scivolo against scalac 3.9.0 on real codebases (#244).

Usage: scripts/bench_compile.py CODEBASE [RUNS]

CODEBASE is one of CODEBASES or `all`. All files of a codebase are compiled as one unit, with the
same sources and classpath, by scivolo-jvm, scivolo-lower (Native), scalac on a fresh JVM (cold)
and scalac WARM_RUNS times in one JVM (warm, scripts/ScalacWarm.java), each RUNS times, the tools
alternating; scivolo compiles scalalib as two units, its files of source version `2.13` and
the others, its times summed. Prints and writes to tmp/bench-compile/CODEBASE.md, per tool: files refused, wall and
user+sys CPU seconds (min of the runs; warm: of the runs after WARMUP), peak RSS, and the ratios
scalac / scivolo. The machine may be loaded: CPU time and minima are the numbers to read. A
refused file is left out of the unit and the rest compiled again, so its codebase's scivolo times
are not comparable.
"""
import importlib
import os
import re
import shutil
import subprocess
import sys
import time
import zipfile
from dataclasses import dataclass, field
from pathlib import Path

import jvm_lib_compile
import jvm_lib_oracle

lib = importlib.import_module("lib-compile")

ROOT = Path(__file__).resolve().parent.parent
WORK = ROOT / "tmp/bench-compile"
RELEASE = ROOT / "target/release"
LIBRARY = ROOT / "ref/libs/scala-library-3.9.0.jar"
WARM_RUNS, WARMUP = 8, 3
# The classes of Scala Native's modules from `just lib-oracle`'s sbt build, which scalac's macro
# calls (`stackalloc`) load.
SN_CLASSES = ROOT / "tmp/lib/build/scala-native/{}/.3/target/scala-3.9.0/classes"
UPICKLE = "com.lihaoyi:upickle-core_{}:4.4.3"
GENY = "com.lihaoyi:geny_{}:1.1.1"


@dataclass
class Codebase:
    """Sources under `root` and the arguments of each tool; a tool without arguments is not run.
    scivolo compiles the groups of `units` one after the other, each as one unit, if given."""
    root: Path
    files: list
    jvm: list | None = None
    lower: list | None = None
    lower_env: dict = field(default_factory=dict)
    scalac: list | None = None
    units: list | None = None


def coursier(*args):
    r = subprocess.run(["coursier", "fetch", *args], capture_output=True, text=True, check=True)
    return r.stdout.split()


def scala_files(root, dirs=(".",)):
    return sorted(str(f.relative_to(root)) for d in dirs for f in (root / d).rglob("*.scala"))


def tasty_core():
    root = ROOT / "ref/scala3/tasty/src"
    return Codebase(root, scala_files(root), [], [], scalac=["-classpath", str(LIBRARY)])


def upickle_core():
    root = WORK / "upickle-core/src"
    if not (root / ".done").exists():
        with zipfile.ZipFile(coursier("--sources", "--default=false", "--intransitive", UPICKLE.format(3))[0]) as z:
            z.extractall(root, [n for n in z.namelist() if n.endswith(".scala")])
        (root / ".done").touch()
    geny, native = coursier("--intransitive", GENY.format(3))[0], coursier("--intransitive", GENY.format("native0.5_3"))[0]
    return Codebase(root, scala_files(root), ["--cp", geny], ["--tasty", native],
                    scalac=["-classpath", f"{LIBRARY}:{geny}"])


def scalalib():
    root = jvm_lib_oracle.sources(LIBRARY.parent / "scala-library-3.9.0-sources.jar", ROOT / "tmp/lib-jvm/scalalib/sources")
    java = sorted(str(f.relative_to(root)) for f in root.rglob("*.java"))
    scalac = ["-classpath", "", "-sourcepath", ".", *jvm_lib_oracle.LIBRARY_OPTIONS, *java]
    files = scala_files(root)
    scala2 = [f for f in files if "import scala.language.`2.13`" in (root / f).read_text()]
    units = [scala2, [f for f in files if f not in scala2]]
    return Codebase(root, files, jvm_lib_compile.OPTIONS, scalac=scalac, units=units)


def javalib():
    if not lib.ORACLE_JARS.is_dir() or not Path(str(SN_CLASSES).format("nativelib")).is_dir():
        sys.exit("javalib needs Scala Native's build of its libraries: run `just lib-oracle`")
    root = lib.SN / "javalib/src/main"
    classes = [str(SN_CLASSES).format(m) for m in ("nativelib", "clib", "posixlib", "windowslib")]
    return Codebase(root, scala_files(root, [d for d in lib.SOURCE_DIRS if (root / d).is_dir()]),
                    lower=["--macros", *lib.OPTIONS["javalib"]], lower_env={"SCIVOLO_LIBS": str(lib.ORACLE_JARS)},
                    scalac=["-classpath", ":".join([str(LIBRARY), *classes])])


CODEBASES = {"tasty-core": tasty_core, "upickle-core": upickle_core, "scalalib": scalalib, "javalib": javalib}


@dataclass
class Run:
    """One compilation: wall and CPU seconds, peak RSS in MB, files refused (None: the run failed)."""
    wall: float
    cpu: float
    rss: float
    refused: int | None


def timed(cmd, cwd, env, log):
    """The Run of `cmd`, its output appended to file `log`."""
    with open(log, "a") as out:
        start = time.perf_counter()
        p = subprocess.Popen(cmd, cwd=cwd, env=os.environ | env, stdout=out, stderr=subprocess.STDOUT)
        _, status, usage = os.wait4(p.pid, 0)
        wall = time.perf_counter() - start
    text = log.read_text(errors="replace")
    refused = len(re.findall(r"^\S+\.scala: ", text, re.M))
    ok = os.waitstatus_to_exitcode(status) == 0 or refused > 0
    rss = usage.ru_maxrss / (1 << 20 if sys.platform == "darwin" else 1 << 10)
    return Run(wall, usage.ru_utime + usage.ru_stime, rss, refused if ok else None)


def timed_all(cmds, cwd, env, log):
    """The Run of the commands of `cmds` (each with its number of files) run one after the other:
    times summed, the largest RSS, refusals counted afresh; a unit that fails counts as refused
    where there are several."""
    log.write_text("")
    runs = [(timed(cmd, cwd, env, log), files) for cmd, files in cmds]
    if len(runs) == 1 and runs[0][0].refused is None:
        return runs[0][0]
    refused = len(re.findall(r"^\S+\.scala: ", log.read_text(errors="replace"), re.M))
    refused += sum(files for r, files in runs if r.refused is None)
    return Run(sum(r.wall for r, _ in runs), sum(r.cpu for r, _ in runs), max(r.rss for r, _ in runs), refused)


def scalac_classpath():
    return coursier("--classpath", jvm_lib_oracle.DOTC)[0]


def warm_driver(cp):
    """The directory of ScalacWarm's class file, compiled once."""
    out = WORK / "warm-driver"
    if not (out / "ScalacWarm.class").exists():
        subprocess.run(["javac", "-cp", cp, "-d", str(out), str(ROOT / "scripts/ScalacWarm.java")], check=True)
    return out


def commands(c, out, cp):
    """The command lines and environment of each tool compiling codebase `c` into `out`/TOOL."""
    java = ["java", "-Xss8m", "-cp"]
    scalac = lambda tool: [*c.scalac, "-d", str(out / tool), *c.files]
    units = list(enumerate(c.units or [c.files]))
    jvm = lambda: [([str(RELEASE / "scivolo-jvm"), *c.jvm, "-d", str(out / f"scivolo-jvm/{i}"), *fs], len(fs))
                   for i, fs in units]
    lower = lambda: [([str(RELEASE / "scivolo-lower"), *c.lower, *fs, str(out / f"scivolo-lower/{i}")], len(fs))
                     for i, fs in units]
    whole = len(c.files)
    tools = {
        "scivolo-jvm": c.jvm is not None and (jvm(), {}),
        "scivolo-lower": c.lower is not None and (lower(), c.lower_env),
        "scalac cold": c.scalac is not None and ([([*java, cp, "dotty.tools.dotc.Main", *scalac("scalac-cold")], whole)], {}),
        "scalac warm": c.scalac is not None and (
            [([*java, f"{cp}:{warm_driver(cp)}", "ScalacWarm", str(WARM_RUNS), *scalac("scalac-warm")], whole)], {}),
    }
    return {t: v for t, v in tools.items() if v}


def warm_run(run, log):
    """`run` with the wall and CPU seconds of the fastest of ScalacWarm's runs after WARMUP."""
    times = [tuple(int(x) / 1e9 for x in l.split()) for l in log.read_text().splitlines() if re.fullmatch(r"\d+ \d+", l)]
    if run.refused is None or len(times) < WARM_RUNS:
        return Run(0, 0, run.rss, None)
    return Run(min(w for w, _ in times[WARMUP:]), min(c for _, c in times[WARMUP:]), run.rss, 0)


def best(runs):
    ok = [r for r in runs if r.refused is not None]
    if not ok:
        return None
    return Run(min(r.wall for r in ok), min(r.cpu for r in ok), min(r.rss for r in ok), ok[0].refused)


def bench(name, runs):
    c = CODEBASES[name]()
    out, cp = WORK / name, scalac_classpath()
    found = {t: [] for t in commands(c, out, cp)}
    for _ in range(runs):
        for tool, (cmds, env) in commands(c, out, cp).items():
            shutil.rmtree(out / tool.replace(" ", "-"), ignore_errors=True)
            (out / tool.replace(" ", "-")).mkdir(parents=True)
            log = out / f"{tool.replace(' ', '-')}.log"
            run = timed_all(cmds, c.root, env, log)
            found[tool].append(warm_run(run, log) if tool == "scalac warm" else run)
    report(name, len(c.files), len(c.units or [c.files]), runs, {t: best(rs) for t, rs in found.items()}, out)


def report(name, files, units, runs, results, out):
    """Prints the table of `results` and writes it to WORK/NAME.md."""
    unit = "one unit" if units == 1 else f"one unit (scivolo: {units} units one after the other, times summed)"
    lines = [f"{name}: {files} files as {unit}, min of {runs} runs (warm: of runs {WARMUP + 1}-{WARM_RUNS} "
             "in one JVM); the machine is shared, read CPU time", "",
             "| tool | refused | wall s | CPU s | peak RSS MB | scalac cold / tool (wall, CPU) | scalac warm / tool (wall, CPU) |",
             "|---|--:|--:|--:|--:|--:|--:|"]
    cold, warm = results.get("scalac cold"), results.get("scalac warm")
    for tool, r in results.items():
        if r is None:
            lines.append(f"| {tool} | failed (see {(out / tool.replace(' ', '-')).relative_to(ROOT)}.log) | | | | | |")
            continue
        ratio = lambda s: f"{s.wall / r.wall:.1f}×, {s.cpu / r.cpu:.1f}×" if s and tool.startswith("scivolo") else ""
        lines.append(f"| {tool} | {r.refused} | {r.wall:.2f} | {r.cpu:.2f} | {r.rss:.0f} | {ratio(cold)} | {ratio(warm)} |")
    text = "\n".join(lines) + "\n"
    (WORK / f"{name}.md").write_text(text)
    print(text)


if __name__ == "__main__":
    if len(sys.argv) not in (2, 3) or sys.argv[1] not in [*CODEBASES, "all"]:
        sys.exit(__doc__)
    for n in CODEBASES if sys.argv[1] == "all" else [sys.argv[1]]:
        bench(n, int(sys.argv[2]) if len(sys.argv) == 3 else 3)
