# Spike 00: Baseline

This spike measures how long it takes today to run small Scala 3 scripts, so we know what latency a Rust-hosted
interpreter has to beat. It also records the sizes of the TASTy, class and NIR files and of the stdlib jars.

## Environment

| | |
|---|---|
| Machine | Apple M4 Pro (arm64), 12 cores, 48 GiB, macOS 15.7.9 (Darwin 24.6.0) |
| Scala | **3.9.0** (published on Maven Central; RC1–RC6 also exist; scala-cli 1.14.0 defaults to 3.8.3, so the scripts pin `//> using scala 3.9.0`) |
| scala-cli | 1.14.0 (the `scala` runner is 1.9.1). Bloop is the build server. |
| Scala Native | **0.5.12**. scala-cli 1.14.0 defaults to 0.5.11, which has no `nscplugin_3.9.0`, so `--native-version 0.5.12` is required. |
| JVM | OpenJDK 21.0.12 (Homebrew) |
| Others | Python 3.14.6, rustc 1.95.0, Apple clang 17 |

This is a developer laptop with IDEs open, so expect some noise. The numbers below come from a clean second pass
(stddev is under 5% for most rows). The exact values are in `results/bench.txt` and `results/bench.csv`.

## Layout / how to reproduce

```
scripts/        hello.scala, moderate.scala (~60 lines: enum, case classes, sealed trait, pattern matching,
                groupBy/groupMapReduce/foldLeft, s/f interpolators), cpu.scala (naive fib(30) + sorting 1e5 Ints)
                plus Python and Rust equivalents for reference
bench.py        small hyperfine stand-in (hyperfine isn't installed): warmup, N runs, --prepare, CSV output
run.sh          ./run.sh [all|env|bench|artifacts]   (RUNS=10, SLOW_RUNS=5 by default)
results/        env.txt, bench.txt, bench.csv, artifacts.txt
artifacts/      moderate.scala + Employee.tasty, moderate$package.tasty, Employee.nir (from Scala 3.9.0 / SN 0.5.12)
work/           scratch (gitignored)
```

Exact commands used by `run.sh`. `F` is a copy of the script in `work/<name>/`, and "changed src" means
`--prepare 'echo "// $(uuidgen)" >> F'` runs before every timed run:

| id | command |
|---|---|
| (a) | `scala-cli run F` after changing the source (warm Bloop does an incremental recompile) |
| (a0) | `scala-cli --power bloop exit`, change the source, then `scala-cli run F` (fully cold: daemon JVM start + cold compiler) |
| (b) | `scala-cli run F` with the source unchanged (cached compilation, warm Bloop) |
| (c) | `scala-cli run --server=false F`. scala-cli forks `java … dotty.tools.dotc.Main` **on every run**, because this mode has no incremental cache (unchanged and changed source cost the same) |
| (d) | `java -cp app.jar:scala3-library_3-3.9.0.jar:scala-library-3.9.0.jar <main>`. The classpath comes from `scala-cli --power compile --print-class-path` |
| (d') | same as (d) plus `-XX:+AutoCreateSharedArchive -XX:SharedArchiveFile=…jsa` (dynamic AppCDS; CDS only accepts jars, which is why the classes are jarred) |
| (e) | `scala-cli --power package --native --native-version 0.5.12 --native-mode {debug,release-fast} F -o bin -f`, then run `bin` |
| (f) | `python3 -c 'print(...)'`, `python3 moderate.py/cpu.py`, `rustc -O` binaries, a plain Java hello, and `/usr/bin/true` |

## Results (wall-clock mean in ms; ≥5 runs, 10 for fast commands)

| | hello | moderate | cpu (fib 30 + sort 1e5) |
|---|---:|---:|---:|
| (a0) scala-cli run, Bloop **stopped** + changed src | **1829** | – | – |
| (c) scala-cli run `--server=false` (always recompiles) | 1033 | 1741 | 1360 |
| (a) scala-cli run, changed src, warm Bloop | 362 | 521 | 413 |
| (b) scala-cli run, cached, warm Bloop | 292 | 329 | 349 |
| scala-cli compile, changed src, warm Bloop | 270 | 359 | 291 |
| scala-cli compile, cached, warm Bloop (no-op) | 233 | 235 | 228 |
| scala-cli launcher only (`scala-cli version --offline`) | 14 | | |
| (d) `java -cp` precompiled | 69 | 109 | 125 |
| (d') `java` + AppCDS | 31 | 59 | 78 |
| (e) Scala Native build, debug (changed src) | 3739 | 4608 | 3377 |
| (e) Scala Native build, release-fast (changed src) | 4218 | 5594 | 4378 |
| (e) Scala Native binary, debug | 4.2 | 4.3 | 57 |
| (e) Scala Native binary, release-fast | 4.8 | 5.1 | 22 |
| (e) `scala-cli run --native`, cached | 296 | 303 | 347 |
| (f) python3 (`-c` / script) | 18 | 26 | 113 |
| (f) Rust binary (`rustc -O`) | 4.1 | – | 7.0 |
| (f) plain Java hello (`java -cp`) | 25 | – | – |
| (f) `/usr/bin/true` (spawn floor of the harness) | 3.6 | – | – |

Scala Native binaries are 1.7–3.7 MB. The Rust binaries are about 0.47 MB.

## Artifacts (moderate.scala, 2382 bytes of source)

| artifact | files | bytes |
|---|---:|---:|
| `.tasty` | 8 | 21 245 (the largest is `moderate$package.tasty` at 8.4 KB, `Employee.tasty` is 2.9 KB) |
| `.class` | 15 | 42 742 |
| `.nir` (Scala Native 0.5.12) | 35 | 54 964 |

Stdlib and runtime jars (uncompressed entry sizes; see `results/artifacts.txt`):

| jar | on disk | contents |
|---|---:|---|
| `scala-library-3.9.0.jar` | 9.8 MB | **941 `.tasty` (7.56 MB)** + 3662 `.class` (15.7 MB) |
| `scala3-library_3-3.9.0.jar` | 344 B | empty shim (only a manifest). Since 3.8 the stdlib is a single `scala-library` artifact **compiled by Scala 3, with TASTy for the whole stdlib** (the 2.13 collections included) |
| samples | | `Predef.tasty` 39.6 KB, `Option.tasty` 28.2 KB, `collection/IterableOps.tasty` 53.0 KB, `immutable/List.tasty` 24.5 KB, `immutable/Vector.tasty` 21.5 KB |
| `scalalib_native0.5_3-3.9.0+0.5.12.jar` | 7.6 MB | 4982 `.nir` (13.7 MB), no TASTy |
| `javalib_native0.5_3-0.5.12.jar` | 6.3 MB | 3503 `.nir` (10.2 MB), a Scala reimplementation of `java.*` |
| nativelib / clib / posixlib / windowslib | 5.6 MB total | NIR + class + TASTy |

The JDK types used by Scala code (`java.lang.String`, boxing, `Math`, …) have **no TASTy**. An interpreter has to get
them some other way: built-in intrinsics, reading JDK class files or `ct.sym`, or using `javalib_native`'s NIR or
TASTy sources as a reference.

## Where the time goes in scala-cli

For `hello`, cached (292 ms):
- **~14 ms**: the scala-cli launcher, a GraalVM native image. This part is cheap.
- **~220 ms**: scala-cli's input and directive processing, the BSP connection to Bloop, and Bloop's no-op compile
  (`scala-cli compile`, cached, takes 228–235 ms whatever the script). This is **fixed overhead on every
  invocation** and it is the biggest single item.
- **~70 ms**: forking the program JVM and running it. That is ~25 ms of plain JVM startup (a Java hello takes
  25 ms) plus **~45 ms to load and initialize the Scala stdlib** (Predef, collections). AppCDS brings the Scala hello
  down to 31 ms.

After an edit (a), a **warm** Bloop compiler adds about 70 ms (hello, cpu) to 190 ms (moderate). A warm dotc is fast.
- Without a warm compiler, the cost is JIT warmup of dotc. `--server=false` costs 1.0–1.7 s every time, and a stopped
  Bloop costs about 1.8 s for hello (daemon start plus cold compile). The first run in a fresh shell or after a
  reboot or an upgrade looks like this.
- `scala-cli run --native` (cached) costs about the same as the JVM path (~300 ms) because the fixed scala-cli and
  Bloop overhead dominates. The native binary takes only 4–5 ms, but every edit costs 3.4–5.6 s of linking and LLVM,
  so Scala Native is not an option for an edit-run loop.

For CPU-bound work, the JVM JIT is good. `cpu` does about 55 ms of work on top of the ~70 ms startup. Compare
Scala Native release-fast at 22 ms total, Rust at 7 ms, and CPython 3.14 at 113 ms.

## Conclusions: the latency budget

An interpreter is always in the edit situation, because it re-reads the source every time. The fair comparison is
therefore (a) at **360–520 ms**. In the common "already warm" case scala-cli costs at least **~290 ms**, and a
cold start costs **1–2 s**.

1. **Hard ceiling (no win at all): about 250 ms** end to end for the moderate script, parse and type-check
   included. Above that, scala-cli with a warm Bloop is just as good.
2. **A clear win is ≤ 50 ms** for hello and the moderate script (process start, stdlib metadata, frontend and
   execution together). That is 7–10× faster than the scala-cli edit-run loop, 35× faster than a cold scala-cli, and
   faster than even `java -cp` on precompiled classes (69–109 ms). **The stretch goal is ≤ 20–25 ms, the same as
   `python3`** (18 ms for hello, 26 ms for moderate). Scala Native and Rust show that the process floor is about 4 ms.
3. **Stdlib metadata is the main risk for startup.** `scala-library` ships 7.56 MB of TASTy in 941 files.
   Unpickling it eagerly would use up the whole budget. We need lazy per-symbol loading from an mmapped index, or a
   pre-digested snapshot of the stdlib built once, like CDS or a Python `.pyc`. The moderate script's own TASTy is
   only 21 KB, so user code is small.
4. **For compute, CPython is the realistic target.** CPython takes 113 ms for fib(30) plus a 1e5 sort. An
   interpreter at that speed still beats scala-cli (349–413 ms) on `cpu`. Scripts would need to be about 3× slower
   than CPython on that workload before scala-cli wins, and long-running compute will always favour the JVM, Scala
   Native or a later JIT. The sweet spot is short scripts, where startup and frontend costs dominate.
5. Because the scala-cli overhead is fixed, even a trivial `hello` never gets below ~290 ms with scala-cli today.
   The interpreter's value is removing that roughly 300 ms floor and the 1–2 s cold path.

Notes: the JVM `f"…"` interpolator uses the default locale (on this machine it prints `115000,00`), while Python
prints `115000.00`. The outputs are otherwise identical across Scala, Python and Rust.
