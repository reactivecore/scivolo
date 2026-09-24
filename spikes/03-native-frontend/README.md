# Spike 03: a native (JVM-free) dotc as frontend: scalino

Question: can a Scala 3.9 compiler running natively, with no JVM, act as the frontend of our interpreter? The
interpreter would take TASTy from it. The spike looks at the per-script latency, binary size and maintenance risk.

## What scalino is (verified 2026-09-24)

| | |
|---|---|
| Repo | **github.com/lolgab/scalino** (Lorenzo Gabriele; created 2026-09-02, 22 stars, 240 commits, last push 2026-09-24). `coreyoconnor/scalino` is a fork (Corey O'Connor contributes Nix packaging). |
| License | Apache-2.0. The patched scala3 and scala-native sources are Apache-2.0 too. The release also ships `java.base.jar`, which holds class files extracted from a JDK (GPLv2+CE). |
| Pins (`versions.env`) | Scala **3.9.0**, Scala Native **0.5.12**. That is exactly our target. |
| What it is | A "mini scala-cli" (`scalino run/compile/package/test`), plus `scalino-dotc` (dotc 3.9.0 + nscplugin, **compiled by itself to Scala Native**), `scalino-linkdriver` (NIR → clang) and `scalino-lsp`. The only thing it needs at run time is clang. |
| Patches | 19 against scala3 and **42 against scala-native** (GC, SMI tagged boxing, direct LLVM codegen, thread fixes, …). Macros are expanded by scalino's own TASTy tree interpreter in `dotc/quoted/Interpreter.scala`. `docs/findings.md` (2,000 lines) is a day-by-day log that includes open bugs (for example a `ForkJoinPool`/`park` race found on 09-22). |
| Releases | v0.0.1 … **v0.0.13** (2026-09-23): 13 releases in 16 days, with tarballs for macOS arm64/x86_64, Linux, deb and rpm. **We used the macOS arm64 binary release; nothing was built from source.** |

Release sizes (v0.0.13, macOS arm64): 145 MB tgz. `scalino-dotc` **42.5 MB** (links only libSystem and libz),
`scalino-linkdriver` 89 MB, `scalino-lsp` 48 MB, `scalino-cs` 62 MB, `lib/` 55 MB of jars, `java.base.jar` 12.6 MB.
For a frontend you only need `scalino-dotc` + `java.base.jar` + `scala-library-3.9.0.jar`, about 65 MB.

## How to reproduce

```
./run.sh fetch     # downloads and checks work/scalino-v0.0.13-macos-arm64.tar.gz
./run.sh bench     # results/bench.txt|csv   (uses ../00-baseline/bench.py)
./run.sh macros    # results/macros.txt
./run.sh rss       # results/rss.txt (not run yet, see RSS below)
```

The TASTy-only command, which is what the interpreter would call:

```
scalino-dotc -javabootclasspath java.base.jar -classpath scala-library-3.9.0.jar \
  -Ystop-after:firstTransform -Xearly-tasty-output OUT -d OUT script.scala
```

Why this particular form:
- Early TASTy is written asynchronously. With `-Ystop-after:pickler`, `inlining` or `pickleQuotes`, **no `.tasty`
  is ever written** (0 files in 20 of 20 runs). `firstTransform` is the earliest stop that writes it (8 of 8 files
  in 20 of 20 runs), and it costs only a few ms more than stopping at `typer`. This is upstream dotc behaviour, not
  something scalino introduced.
- `-javabootclasspath java.base.jar` is required because a native binary has no `jrt:/` filesystem.
- To emit NIR instead, pass `-classpath <nativelibs.cp> -Xplugin:nscplugin.jar -Xplugin-require:scalanative
  -Yretain-trees`. The plugin is compiled into the binary and the flag only switches it on.

## Results (M4 Pro, wall-clock mean in ms, new process for every run; n=10, JVM n=5)

| | hello | moderate | cpu |
|---|---:|---:|---:|
| `scalino-dotc -version` (process floor) | 9.8 | | |
| native dotc `-Ystop-after:parser` (startup + compiler init) | 39 | 39 | 41 |
| native dotc `-Ystop-after:typer` | 55 | 172 | 135 |
| **native dotc TASTy only** (`firstTransform` + early output) | **60** | **198** | **150** |
| native dotc full NIR (nscplugin) | 228 | 526 | 372 |
| JVM dotc TASTy only, cold `java` (same jars, same flags) | 683 | 1115 | 883 |
| `scalino run`, source changed (compile + link + run) | 2146 | 3229 | 2629 |
| `scalino run`, cached (runs the linked binary) | 32 | 37 | 102 |
| baseline: scala-cli + warm Bloop, changed source (a) | 362 | 521 | 413 |
| baseline: scala-cli `--server=false` (c) | 1033 | 1741 | 1360 |

Macro scripts (`scripts/macros/`):

| | ms |
|---|---:|
| `compiletime.scala` native TASTy only | 178 |
| `compiletime.scala` native full NIR | 745 (sd 290, noisy) |
| `compiletime.scala` JVM TASTy only, cold | 1059 |
| `Macro.scala` + `UseMacro.scala` native TASTy only, same run | 235 |
| `Macro.scala` + `UseMacro.scala` native full NIR, same run | 484 |

A cold JVM dotc cannot do the same-run quoted macro with `-Ystop-after`: it fails with "Cyclic macro dependencies"
because it has to execute the macro's bytecode. scalino interprets the macro's tree instead, so it works.

**RSS** (`/usr/bin/time -l`, one manual run on moderate): about **99 MB** TASTy-only and **218 MB** full NIR. The
`rss` target was not run before the session ended.

## Macros (results/macros.txt)

| case | result |
|---|---|
| `scala.compiletime`: `inline match`/`erasedValue`, `constValue`, `summonInline`, `codeOf`, `ops.int`, `Mirror`-based `derives` | **works**, output matches the JVM |
| quoted macro (`${…}`, `'{…}`, `valueOrAbort`, splice recursion) in a separate file, same compiler run | **works** |
| the same macro compiled separately (its TASTy on the classpath, `-Yretain-trees`) | **works** |
| `Expr.show` | **different output**: `x * 2 + 1` wrapped in **ANSI colour codes** (the JVM prints `x.*(2).+(1)`). The codes leak into the program's string, which is a real correctness bug. |
| quote type patterns `case '[Int]`, `case '[List[t]]` | **fail at compile time**: "Interpreter (own implementation) does not support structural quote-type patterns yet". This is a documented gap, and `'[Int]`-style matching is common in real macros. |

Also seen: `scalino run` keeps stale outputs in `.scalino-build/_scratch/classes`. After a different run in the same
directory it reported two entry points, and `rm -rf .scalino-build` fixed it. Every build also prints
`llvm-config: command not found`, which is harmless.

## Findings

1. **Native startup is solved.** The process floor is 10 ms, and parser + compiler init + stdlib symbols cost
   **~39 ms fixed**. A cold JVM dotc spends 600–700 ms on the same thing.
2. **Typer throughput is the problem.** moderate.scala (60 lines) takes ~130 ms of type checking on top of the
   39 ms init, and **198 ms end to end to get TASTy**. That is inside the 250 ms "no-win ceiling" of spike 00, but
   **4× over the ≤ 50 ms target**. Only `hello` comes close (60 ms). Scala Native code has no JIT, and findings.md
   reports the self-hosted build is ~1.5× slower than its earlier GraalVM native-image build on a large project.
3. **The TASTy we get has not been inlined.** Pickling runs before `inlining`, so the `.tasty` still contains
   calls to non-transparent `inline` defs and macros, not their expansions. (This follows from the phase order and
   has not been checked by decoding the TASTy.) An interpreter that reads it must either do its own inlining and
   macro expansion, or dotc must be patched to pickle after `inlining`/`splicing`.
4. **Full NIR plus link is not an edit loop.** Emitting NIR costs 230–530 ms and linking brings `scalino run` to
   2.1–3.2 s after an edit, which is slower than scala-cli with a warm Bloop (360–520 ms). Once linked, a cached
   binary starts in 32 ms.
5. **Maintenance risk is high.** The project is a 3-week-old, single-maintainer effort. It carries 61 patches over
   two upstream compilers and a very high change rate (13 releases in 16 days), and has open concurrency/GC bugs
   and known macro gaps. It only works because Scala and Scala Native are pinned to exactly 3.9.0 / 0.5.12. On the
   other hand, it tracks exactly our versions, uses a permissive license, and is proof that a JVM-free dotc works
   today.

## Verdict

**Not viable as the ≤ 50 ms per-script frontend. Viable as a JVM-free fallback or prototype frontend.**
- Per script: 60 ms (hello), 150–200 ms (cpu, moderate), 180–235 ms with macros. That is about 1.5–2.5× faster than
  scala-cli with a warm Bloop and 5–10× faster than a cold scala-cli, but it misses the target, and ~40 ms of the
  cost is fixed init on every call.
- Binary: 42.5 MB dotc + 12.6 MB `java.base.jar` + the stdlib jar, versus a Rust interpreter of a few MB.
- Risk: a patched, fast-moving fork of the compiler, and macro-interpreter gaps that show up on ordinary macros.

**What a compile server or cache would add:**
- A **content-hash cache** of source → TASTy (or of the interpreter's own IR) makes unchanged scripts free, so the
  cost only comes after an edit. This is easy and worth doing whatever the frontend is.
- A **resident native dotc** (like scalino-lsp, which already keeps a native presentation compiler alive) would
  remove the ~40 ms init and the stdlib symbol loading. It would still pay the ~130 ms of type checking for
  moderate, because native code gets no JIT warm-up. A warm *JVM* dotc (Bloop) type-checks moderate in roughly
  70–190 ms (spike 00), so a JVM server does no better per edit. The ≤ 50 ms goal for an edited script therefore
  needs our own frontend: a Rust parser and typer for the scripting subset (spikes 01/02), with dotc (native or JVM)
  as the fallback for scripts that go beyond that subset.

Not done (session ended): an in-process warm-dotc loop to measure compile-server latency directly, the `rss` target,
decoding the TASTy to confirm point 3, and a GraalVM comparison (no GraalVM is installed).
