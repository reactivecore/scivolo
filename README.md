# scala_interpreter

Research project at Reactive Core: a fast Scala 3.9 interpreter in Rust for
scripting, with no JVM. Status: research and spikes are done; the plan is in
[ROADMAP.md](ROADMAP.md) and the key decision in
[ADR 0001](docs/adr/0001-nir-shaped-execution-ir.md) (proposed).

## What was done (2026-09-24)

| Piece | Where | Outcome |
|---|---|---|
| Compiler pipeline, TASTy, macros | `docs/research/01-…` | TASTy = typed trees with givens/overloads resolved, written *before* inlining and macro expansion. The typer is ~71k lines; don't port it whole |
| Scala Native / Scala.js IRs and test suites | `docs/research/02-…` | NIR is lowered; the whole stdlib + javalib ship precompiled as NIR; a conformance ladder of ~12k tests |
| Interpreter architecture in Rust | `docs/research/03-…` | Closure-compiled IR, Scala Native object layout, Boehm → Immix-like GC, heap snapshots, Rust over C++ |
| Baseline latency | `spikes/00-baseline` | scala-cli: 290 ms (unchanged), 360–520 ms (after an edit), 1–2 s (cold) |
| TASTy reader (Rust) | `spikes/01-tasty-reader` | 941/941 stdlib files, 12.7 ms full decode, 2.3 ms index |
| Parser evaluation (Rust) | `spikes/02-scala-parser` | tree-sitter-scala: 0 errors on the stdlib, but silent misparses → port dotty's parser |
| Native dotc (scalino) | `spikes/03-native-frontend` | Source → TASTy in 60–198 ms without a JVM; macros mostly work; high-risk dependency |
| NIR reader (Rust) | `spikes/04-nir-reader` | 9,677 library files; output matches Scala Native's own printer |
| Minimal TASTy evaluator | `spikes/05-tasty-eval` | Loader only (3.9 ms per process); evaluator unfinished |

Every spike has a README with commands and numbers. The research reports
label each claim as verified [V] or inferred [I], and list their sources.

## Key facts gathered

- **Versions.** Scala 3.9.0 is an LTS release (2026-09-03, TASTy 28.9). Scala
  Native 0.5.12 supports it. scala-cli 1.14 defaults to 3.8.3 and to Scala
  Native 0.5.11, so pin both.
- **Artifacts.** Since 3.8 the stdlib is `scala-library-3.9.0.jar`: 941
  `.tasty` files, 7.56 MB, compiled by Scala 3. `scala3-library_3` is an
  empty stub. On Scala Native the stdlib is in `scalalib_native0.5_3`, and
  `scala3lib_native0.5_3` is the stub.
- **Latency budget.** A clear win is ≤ 50 ms end to end. The floors are 4 ms
  for any process, 25 ms for a JVM hello world, and 18 ms for python3.
  - Loading the user's TASTy costs microseconds.
  - Library loading decides startup. Unzipping alone costs 27 ms for TASTy
    and 130 ms for NIR.
  - So we need an uncompressed, lazily read library image.
- **What TASTy leaves undone.** Plain `inline` calls, macro calls, pattern
  matching, lazy vals, mixins, erasure and `s""` are all unlowered. Most
  expression types are missing too. Local symbols are referenced by address;
  other members by name plus signature.
- **What NIR provides.** It is post-erasure SSA with about 30 operations.
  Exceptions are explicit `unwind` edges, closures are classes, `s""` becomes
  StringBuilder calls, and `match` becomes a `switch`.
  - Raw memory is used by 0.6% of javalib methods and 11% of nativelib.
  - javalib calls 534 distinct external symbols.
- **Macros.** dotc runs macros as JVM bytecode through a class loader.
  scalino shows that a TASTy tree interpreter can replace that. Quote type
  patterns are its known gap.
- **Test suites.**

  | Suite | Size |
  |---|---|
  | SN `unit-tests/shared` | 7,388 @Test (172 of them language tests) |
  | dotty `tests/run` | 2,060 `.scala` files, 880 `.check` files |
  | scala-partest-junit | 2,944 @Test |
  | Scala.js shared suite | 2,713 @Test |

- **Prior art.**
  - scalino: dotc on Scala Native.
  - sjsir-interpreter: an interpreter for Scala.js IR that compiles to
    closures.
  - tasty-query: a model of TASTy symbols and types.
  - Rust JVMs, e.g. ristretto: none is documented to run Scala.

## Status (2026-09-25): milestones M1, M2 and M3 reached

The NIR VM (`crates/vm`, CLI `crates/scalai`) runs Scala 3.9 programs compiled to NIR with
Scala Native's precompiled libraries (scalalib, javalib, nativelib, posixlib, clib, auxlib)
and its C runtime built from `native/`.

- **M1:** hello, moderate, cpu, fib, loops and a runtime test (`tests/programs/`) print exactly
  what their Scala Native builds print.
- **M2:** all of Scala Native's language unit tests pass.
- **M3:** 11,336 of Scala Native's unit tests pass, 3 fail, 21 skip. Per package and the known
  gaps: [docs/conformance.md](docs/conformance.md).
- **Threads:** Java threads run as green threads on coroutines, with monitors, parking,
  timers and blocking C calls on helper OS threads ([ADR 0003](docs/adr/0003-green-threads-first.md));
  `tests/programs/threads` covers `synchronized`, `wait`/`notify`, interrupts, executors and futures.
- **GC:** Scala Native's Immix collector reclaims memory ([ADR 0004](docs/adr/0004-immix-garbage-collector.md));
  `tests/programs/gcstress` allocates far more than it keeps. `GC_MAXIMUM_HEAP_SIZE` and the
  other Immix settings apply.

| Program | scalai (M4 Pro) | scala-cli, unchanged source | Scala Native binary |
|---|---:|---:|---:|
| hello | 9.3 ms | 292 ms | 5 ms |
| moderate | 17 ms | 329 ms | 5 ms |
| cpu (fib 30 + sort 100k) | 0.32 s | 349 ms | 22 ms |

Speed work and ideas: [docs/performance.md](docs/performance.md).

```sh
just fetch-libs                 # download the Scala Native jars into ref/libs once
just libs                       # or copy them from the Coursier cache
just gates                      # fmt, clippy, tests (incl. tests/programs/*)
just run moderate               # run tests/programs/moderate in the release build
just script FILE.sc ARGS        # run a script, compiled through scala-cli unless cached
just check                      # every test program against its .check file, with timings
just bench cpu                  # wall time; `just vm-profile cpu` counts calls and instructions
just program-update NAME        # compile tests/programs/NAME with Scala Native (NIR + .check)
just programs-stale             # programs whose committed NIR or .check a rebuild changes
just overlay                    # compile the library overlay (javalib fixes in Scala, ADR 0009)
just ci-suite                   # Scala Native's unit tests in the VM (CI: on main)
just sn-run Name -Other         # selected Scala Native test classes
```

Scripts run directly: `scalai FILE.sc` or `scalai [--main CLASS] FILE.scala`, also as a
`#!/usr/bin/env scalai` shebang. The first run compiles the file through scala-cli
(`SCALAI_SCALA_CLI` names another binary) and caches its NIR in `SCALAI_CACHE`/`scripts`, keyed
by the file's path and content; later runs start no JVM. Files it pulls in with
`//> using file` and changed dependencies do not invalidate the cache. Compiled scripts and
library images unused for 30 days are removed when the next one is built.

Exceptions carry stack traces with file and line from the interpreter's frames.
`SCALAI_TRACE=1` traces exceptions, `SCALAI_PROFILE=1` counts calls, `SCALAI_TIMEOUT=N`
stops a run.

Decisions (see ROADMAP.md): ADR 0001 accepted; scala-cli compiles NIR until the own frontend
exists; stay on NIR 0.5.x and port to 0.6; green threads first (ADR 0003), OS threads later;
targets are macOS arm64 and Linux x86-64, both tested in GitLab CI (`just ci`). ADR 0002
records the memory trust model.

Next, by expected value:
1. Interpreter speed beyond the phase 1 target: boxed equality, superinstructions,
   polymorphic inlining (docs/performance.md, ADR 0006).
2. Startup: a heap snapshot of the initialized runtime after a training run (hello 9.3 ms
   today).
3. Phase 3, the own frontend, starting with the parser.

Work is tracked in the GitLab issues.

Borrowed code and ideas are credited in [acknowledges.md](acknowledges.md).

## Layout

```
ROADMAP.md          phases, milestones, next steps, open decisions
docs/research/      three research reports
docs/adr/           architectural decisions
spikes/NN-*/        throwaway experiments, each with README.md
CLAUDE.md           project rules (Rust style from rcui)
```

Build folders and downloaded toolchains (`spikes/*/work/`, `target/`) are not
checked in. Each spike's README says how to recreate them.
