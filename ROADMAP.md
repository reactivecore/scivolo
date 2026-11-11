# Roadmap — a fast Scala 3.9 interpreter in Rust

Goal: run Scala 3.9.0 scripts with no JVM, **≤ 50 ms end to end after an
edit** (stretch goal: 20 ms, which is python3's startup). Coverage should
equal the Scala Native subset and be proven with the suites Scala Native
passes. Precompiled libraries must be usable. A JVM bytecode backend shares the frontend.

Evidence for every number below is in `docs/research/` and `spikes/*/README.md`.
The architectural decision is [ADR 0001](docs/adr/0001-nir-shaped-execution-ir.md); the
data flow is in the README's "How it works". `README.md` summarizes the goals and the
progress: update it with this file.

## Phase 0 — research and spikes (done, 2026-09-24)

- [x] Baseline latency (`spikes/00-baseline`). scala-cli with warm Bloop takes
      290–520 ms, 1–2 s without it. Scala Native binaries start in 4 ms but
      take 3.4–5.6 s to build.
- [x] TASTy reader in Rust (`spikes/01`): 941/941 stdlib files decode in
      12.7 ms; header, names and section index alone take 2.3 ms.
- [x] Parser evaluation (`spikes/02`): tree-sitter-scala misparses some input
      silently. Port dotty's parser instead.
- [x] Native dotc via scalino (`spikes/03`): source to TASTy takes 60–198 ms
      and macros mostly work. Too slow for the primary path; not used.
- [x] NIR reader in Rust (`spikes/04`): 9,677 library files decode. On
      `moderate.scala` the output matches Scala Native's own printer byte for
      byte.
- [~] Minimal TASTy evaluator (`spikes/05`): superseded by ADR 0001, but
      documents what TASTy lacks for execution.

## Phase 1 — NIR VM: "run what Scala Native runs" (done)

`crates/vm` interprets NIR with Scala Native's precompiled libraries and its C runtime built
from `native/` (`crates/snrt`), through a library image (ADR 0005). Immix collects memory
([ADR 0004](docs/adr/0004-immix-garbage-collector.md)); Java threads run as green threads
([ADR 0003](docs/adr/0003-green-threads-first.md)); stack traces come from the interpreter's
frames. Results per suite: [docs/conformance.md](docs/conformance.md).

| M | Suite | Status |
|---|---|---|
| M1 | hello, `moderate.scala`, `cpu.scala` | pass |
| M2 | SN `unit-tests/shared` language tests | pass |
| M3 | SN unit tests | 11,336 pass, 3 fail, 21 skip |
| M4 | dotty `tests/run`, compiled by Scala Native | fail only where Scala Native fails |
| M5 | scala/scala `test/junit` | all 867 tests of the files that compile with Scala 3 |

Exit criteria (met): M2 passes, M3 above 80%, a cached hello ≤ 20 ms (9.3 ms), `cpu.scala`
within 3× of CPython (0.26 s against 113 ms), on the M4 Pro reference machine.

Open: OS threads (a concurrency-safe heap, caches and callback table).

## Phase 2 — startup and speed (in part)

Measurements and the ranked list of speed ideas: [docs/performance.md](docs/performance.md).
Done: the library image (ADR 0005), per-site caches, fused instructions, tier-up with guarded
inlining (ADR 0006).

- **Heap snapshot** (done, #9, ADR 0034): the initialized runtime's heap, written after a
  training run and restored at start, also for the REPL; hello 7.5 ms, moderate 16.6 ms
  (minima). Next: decoded definitions and lowered code in the snapshot.
- **Precise roots.** Typed slot maps would make frame scanning precise (ADR 0004); MMTk stays
  the alternative collector.
- **Optional JIT.** A Cranelift baseline tier for hot methods.

## Phase 3 — own Rust frontend: no JVM, no compile step

Exit criterion (met): editing and then running `moderate.scala` takes 48 ms with no
JVM (`just bench-edit`). Measured by dotty's `tests/run` through typer, lowering and VM
(`just dotty-lower-report`, 2026-11-11): 1,496 of 1,711 pass; 10 stop at the typer, 32 fail in
the VM.

1. **Parser** (`crates/syntax`): ports of dotty's `Scanners`, `Parsers`, `Desugar` and
   `DesugarEnums`. Tokens match dotty on all 5,893 files, trees and desugaring on 5,891
   (`just syntax-report`). Open: `-Wconf`.
2. **Symbols from TASTy** (`crates/tasty`, [research 08](docs/research/08-tasty-symbols.md)):
   lazily loaded symbols and types of the standard library and javalib, subtyping, erasure,
   bridges; 99.8% of the library's signatures match Scala Native's mangled names.
3. **Typer for the scripting subset** (`crates/typer`): inference, overloads, implicits and
   givens, extension methods, unions, inlining (ADR 0020), match types, mirrors and `derives`.
   `just typer-report` (2026-11-08): tests/pos 2,361 typed as dotty, 186 differ (16 only in
   warnings), 478 refused. Unsupported
   input is refused, never compiled silently wrong.
4. **Lowering** (`crates/lower`): dotty's phases from FirstTransform to Constructors and
   nscplugin's NIR generation; every lowered test program matches Scala Native's NIR byte for
   byte (`tests/programs/lowered.txt`, `just nir-cmp`).
5. **REPL** (`crates/repl`, ADR 0012): matches dotty 3.9 on the transcripts in `tests/repl`
   (`just repl-oracle`) but for the typer's error messages.
6. **Differential testing.** Run M2–M5 through the own frontend and through dotc, and compare.

## Phase 4 — macros without a JVM (started)

Status (#40, ADR 0029): macro calls expand in scripts, the REPL and native
executables. A library macro runs from its NIR in the VM; its quotes are unpickled into the
typer's trees. Macro libraries compile from source, their quotes pickled to TASTy as dotty does
([research 43](docs/research/43-pickle-quotes.md)). Typed-hole quote patterns and a
`quotes.reflect` subset work; the rest of `Quotes` is refused by name. Macro annotations expand
in the inlining (ADR 0039). Circe 0.14.10's `derives Codec.AsObject` and the com-lihaoyi
libraries work. dotty's `tests/run-macros`: 308 of 321 pass
(`just dotty-lower-report "" run-macros`).

## Phase 5 — precompiled code and wider reach

- **Native executables** (done, #113, [ADR 0028](docs/adr/0028-native-executables-via-cranelift.md)):
  `scivolo-aot` builds sources or NIR into single-threaded executables with Cranelift; 308 of 318
  test programs from source (2026-11-11) (`just aot-src-all`).
- **Third-party libraries** (done): `--dep` loads `_native0.5_3` jars as NIR plus TASTy, fetched
  by Coursier.
- **AOT for libraries.** Compile hot stdlib NIR ahead of time, callable from interpreted code.
- **JVM bytecode backend** (done in its first scope, [ADR 0101](docs/adr/0101-jvm-backend-crates.md),
  [research 46](docs/research/46-jvm-backend.md)): `scivolo-jvm` writes class version 61 with
  stack map frames, TASTy per top-level class
  ([ADR 0105](docs/adr/0105-jvm-tasty-from-the-typer.md)) and jars scala-cli compiles against;
  JDK classes and members javalib lacks come from the JDK's class files (ADR 0106–0108). dotty's
  `tests/run`, `tests/run-macros` (macros run from a Scala Native build, ADR 0112) and
  `tests/programs` on the JVM: docs/conformance.md. Open:
  - what ADR 0107 and 0108 do not enter: nested and annotation classes, `sun.*`;
  - the crash and JVM refusals listed in docs/conformance.md;
  - TASTy as dotty's: type sharing, positions, the trees ADR 0105 refuses;
  - JVM jars on the classpath (`--dep`, scalac's class files) through the class file reader;
  - the standard library as a target; NIR generation out of `crates/lower` into `crates/nirgen`.

  Compile targets are tracked as issues labelled `compile-target`.

## Next

Compatibility first, in order of the tests it unblocks; startup comes last and must not get
slower meanwhile:
1. **Typer groups** ([research 37](docs/research/37-refusals-after-1023.md)): the 10 typer
   refusals in `tests/run` (see [docs/conformance.md](docs/conformance.md)) and the 478 in
   `tests/pos`; implicit candidates with undetermined type variables (#123).
2. **Macros** (#40, [research 26](docs/research/26-macros-next.md)): the 13 failing
   `tests/run-macros`.
3. **com-lihaoyi toolkit** (#216, [research 31](docs/research/31-lihaoyi-macros.md)): `just
   lib-source NAME` compiles every file of fansi 0.5.1, geny 1.1.1, sourcecode 0.4.4, pprint
   0.9.6, fastparse 3.1.1, mainargs 0.7.8, upickle 4.4.3 (with upickle-core, upickle-implicits,
   ujson and upack) and os-lib 0.11.9-M8; 1,777 of 1,786 NIR files are the same as scala-cli's
   (3 differ only in var order; upickle's own 35 have no scala-cli oracle, as Scala 3.9 reports
   cyclic references in `Api.scala`); 2 differ also from scala-cli compiling the file alone
   (docs/conformance.md). Their `tests/aot` programs pass on our NIR.
4. **Lowering and inlining gaps** ([research 42](docs/research/42-lowering-leftovers-after-1029.md)):
   done; `tests/run` has no lowering refusals left.
5. **Scala Native's library through the own frontend** (#168,
   [research 36](docs/research/36-compiling-the-native-stdlib.md), ADR 0042, 0043, 0046): `just
   lib-compile MODULE` lowers each file and compares its NIR with Scala Native's 3.9.0 build of
   it (`just lib-oracle`), reporting files that differ only in the order of var slots (Scala
   Native takes them from a hash set) as `var-order`. Windows is out of scope.

   | Module | Files lowered | NIR same | var-order | differ |
   |---|---:|---:|---:|---:|
   | javalib | 601 of 601 | 3,413 | 82 | 1 |
   | nativelib | 103 of 103 | 707 | 9 | 3 |
   | auxlib | 15 of 15 | 80 | | 5 |
   | clib | 21 of 21 | 114 | | 1 |
   | posixlib | 67 of 67 | 270 | | |
   | windowslib | 34 | 130 | | |
   | scalalib | 645 of 649 | 4,279 | 17 | 11 |

   scalalib is compared with `just lib-oracle`'s build of it. clib, posixlib and scalalib pass Scala Native's unit tests on our jars (`just lib-jar-report`).
   `unsafe/package{,$}` in nativelib differ only as a joint compilation with
   `UnsafePackageCompat` does. Open: the rest of scalalib.
6. **Startup** (#9): decoded definitions and lowered code in the heap snapshot.

Process notes: each batch is reviewed by a subagent before its MR; `just snapshot-diff programs
lower` shows worsened tests, which needs `tmp/dotty/3.9.0-0.5.12/results.tsv` from `just
dotty-build` and `just dotty-report` in every checkout that runs it; its `lib` oracle compares
`just lib-compile all`.

## Decisions

1. **ADR 0001:** code from NIR, types from TASTy.
2. **Frontend:** the own frontend; scala-cli (JVM) compiles what it refuses (ADR 0021).
3. **Scala Native coupling:** stay on the 0.5.x NIR format and port to 0.6 when it ships.
4. **Threads (ADR 0003):** green threads, also backing Java virtual threads; OS threads later.
5. **Platforms:** macOS arm64 and Linux x86-64; GitLab CI tests both on every MR and on main.

## Risks

- **Version churn.** The NIR format changes with Scala Native 0.6. TASTy
  changes with every Scala minor release, but 3.9 is an LTS.
- **Typer scope creep.** The fallback path has to stay cheap so that partial coverage is
  acceptable.
- **OS threads.** The VM runs Java threads as green threads on one OS thread, and native
  executables are single-threaded, until the heap and caches are concurrency-safe.
