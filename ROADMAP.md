# Roadmap — a fast Scala 3.9 interpreter in Rust

Goal: run Scala 3.9.0 scripts with no JVM, **≤ 50 ms end to end after an
edit** (stretch goal: 20 ms, which is python3's startup). Coverage should
equal the Scala Native subset and be proven with the suites Scala Native
passes. Precompiled libraries must be usable. JVM bytecode is optional.

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
  training run and restored at start, also for the REPL; hello 10.2 → 7.5 ms, moderate 18.6 →
  16.6 ms (2026-10-22, minima). Next: decoded definitions and lowered code in the snapshot.
- **Precise roots.** Typed slot maps would make frame scanning precise (ADR 0004); MMTk stays
  the alternative collector.
- **Optional JIT.** A Cranelift baseline tier for hot methods.

## Phase 3 — own Rust frontend: no JVM, no compile step

Exit criterion (met, 2026-10-15): editing and then running `moderate.scala` takes 48 ms with no
JVM (`just bench-edit`). Measured by dotty's `tests/run` through typer, lowering and VM
(`just dotty-lower-report`): 1,441 of 1,711 pass (2026-10-30); 65 stop at the typer, none at
the lowering, 32 fail in the VM.

1. **Parser** (`crates/syntax`): ports of dotty's `Scanners`, `Parsers`, `Desugar` and
   `DesugarEnums`. Tokens match dotty on all 5,893 files, trees and desugaring on 5,891
   (`just syntax-report`). Open: `-Wconf`.
2. **Symbols from TASTy** (`crates/tasty`, [research 08](docs/research/08-tasty-symbols.md)):
   lazily loaded symbols and types of the standard library and javalib, subtyping, erasure,
   bridges; 99.8% of the library's signatures match Scala Native's mangled names.
3. **Typer for the scripting subset** (`crates/typer`): inference, overloads, implicits and
   givens, extension methods, unions, inlining (ADR 0020), match types, mirrors and `derives`.
   `just typer-report` (2026-10-22): tests/pos 1,803 typed as dotty, 141 differ (20 only in
   warnings), 1,081 refused. Unsupported
   input is refused, never compiled silently wrong.
4. **Lowering** (`crates/lower`): dotty's phases from FirstTransform to Constructors and
   nscplugin's NIR generation; every lowered test program matches Scala Native's NIR byte for
   byte (`tests/programs/lowered.txt`, `just nir-cmp`).
5. **REPL** (`crates/repl`, ADR 0012): matches dotty 3.9 on the transcripts in `tests/repl`
   (`just repl-oracle`) but for the typer's error messages.
6. **Differential testing.** Run M2–M5 through the own frontend and through dotc, and compare.

## Phase 4 — macros without a JVM (started)

Status (2026-10-19, #40, ADR 0029): macro calls expand in scripts, the REPL and native
executables. A library macro runs from its NIR in the VM; its quotes are unpickled into the
typer's trees. Typed-hole quote patterns and a `quotes.reflect` subset work; the rest of
`Quotes` is refused by name. Macro annotations expand in the inlining (ADR 0039). Circe
0.14.10's `derives Codec.AsObject`, sourcecode 0.4.4, fastparse 3.1.1, mainargs 0.7.8, upickle,
pprint and os-lib work. dotty's `tests/run-macros`: 238 of 321 pass (2026-10-30,
`just dotty-lower-report "" run-macros`).

## Phase 5 — precompiled code and wider reach

- **Native executables** (done, #113, [ADR 0028](docs/adr/0028-native-executables-via-cranelift.md)):
  `scivolo-aot` builds sources or NIR into single-threaded executables with Cranelift; 218 of 226
  test programs from source (2026-10-29) (`just aot-src-all`).
- **Third-party libraries** (done): `--dep` loads `_native0.5_3` jars as NIR plus TASTy, fetched
  by Coursier.
- **AOT for libraries.** Compile hot stdlib NIR ahead of time, callable from interpreted code.
- **JVM bytecode frontend (optional).** Needs `invokedynamic`/LambdaMetafactory and JDK
  natives, and runs into the JDK's licence. Low priority.

## Next

Compatibility first, in order of the tests it unblocks; startup comes last and must not get
slower meanwhile:
1. **Typer groups**: [research 37](docs/research/37-refusals-after-1023.md)'s census;
   named-tuple leftovers (#146); match-type leftovers (#120); typed-tree printing gap 2 (#122).
   Done 2026-10-30: printing of library object prefixes and default getters (#156), `IArray`
   indexing (#187), `os.proc` with stdin (#173). Done 2026-10-26: `Selectable` and structural calls, higher-kinded aliases, named tuple
   trees (#146); dependent matches, pattern-bound types, library sealed children (#120, #129);
   erased definitions and saferExceptions (ADR 0054); implicit scope through prefixes,
   ClassTag/Manifest synthesis, polymorphic givens as import bounds (#155, #164); wrong
   overrides refused (#124). Done 2026-10-24: Java enums (§2.7), dependent function
   types, refinements adding type members (#164); extension fallback to the implicit scope,
   i16065 (#166); named tuples, XXL tuples, named and higher-kinded type arguments (#139);
   mirrors through refined paths, match-type reduction through them, `codeOf` (#129, #120).
2. **Macros** (#40, [research 26](docs/research/26-macros-next.md)): the run-macros refusals,
   measured on `tests/run-macros`. Done 2026-10-30: `TypeReprStructure` of packages,
   `TreeStructure` of `Return` and `ClassDef`, curried, dependent and polymorphic higher-order
   holes, local classes from macro annotations (#185). Done 2026-10-26: reflect tree and type operations, TreeMap/ExprMap,
   `typeChecks` (ADR 0053). Done 2026-10-24: macro
   annotations (ADR 0039), `Symbol.newClass`, `newModule`, `newMethod`, `newVal`,
   `ClassDef`/`DefDef`/`ValDef` construction, `Term.etaExpand`.
3. **com-lihaoyi toolkit** (#144, [research 31](docs/research/31-lihaoyi-macros.md)): done
   2026-10-24: upickle derivation of case classes, sealed traits and enums (R7, R8), pprint's
   `tprint`, os-lib paths, as scripts and native executables; earlier fastparse (R2) and
   mainargs (R5, R6; ADR 0037).
4. **Lowering and inlining gaps** ([research 34](docs/research/34-lowering-leftovers.md),
   [research 35](docs/research/35-local-classes-in-inline-bodies.md), #165): done 2026-10-30
   ([research 42](docs/research/42-lowering-leftovers-after-1029.md)): no lowering refusal left in
   `tests/run`; local captures, polymorphic `super` calls, library trait parameters,
   `@static` members of trait companions (#184). Done 2026-10-26: local objects and `super` in inline bodies, members of
   path-dependent aliases seen from the path, local lazy values of value classes, `@static`
   values (ADR 0047). Done 2026-10-24: implicit
   search in the call's context, `summonFrom` givens, polymorphic inline accessors, library
   trait variables, the forward-reference check (#90), local enums, nested context-result
   bridges.
5. **Value classes and statics** (#88): done 2026-10-24: i15725, t13033, `@static` members
   (ADR 0044), `scala.runtime` stubs, exports of inline members, opaque types in classes.
6. **Tooling:** test programs compiled with Scala 3.9.0 explicitly (#91).
7. **Scala Native's library through the own frontend** (#168,
   [research 36](docs/research/36-compiling-the-native-stdlib.md), ADR 0042, 0043): `just
   lib-compile MODULE` lowers each file and compares its NIR with Scala Native's build of it
   (`just lib-oracle`, Scala 3.9.0, ADR 0046; the published jars are Scala 3.1.3's).
   2026-10-26, measured per branch: javalib 387 of 601 files lowered with G4, 380 with G6c
   (977 of 1,063 NIR files identical); nativelib 77 of 103; posixlib 57; windowslib 32.
   Done: G1, G2, G3, G4 (intrinsics, `CFuncPtr`, `c"…"`, link-time conditions, `@struct`,
   ADR 0045), G5, G6a, G6b in part, G6c. Next: extern varargs, `@align`, SafeZone.
   2026-10-27: same-array branches, default `@align`, extern field reads and vararg declarations
   lower; against the 3.9.0 oracle nativelib lowers 88 of 103, clib 18 of 21, posixlib 59 of 67,
   javalib 488 of 601, auxlib 6 of 15. `lib-compile` also
   reports oracle NIR files absent from the generated set. Windows remains out of scope.
   2026-10-29 ([research 41](docs/research/41-native-stdlib-after-1029.md)): JDK stubs for javalib,
   access rules; nativelib lowers 92 of 103, clib 19 of 21, posixlib 66
   of 67, windowslib 34, javalib 531 of 601, auxlib 7 of 15.
   2026-10-30: clib and posixlib complete and pass Scala Native's unit tests on our jars (`just
   lib-jar-report`); nativelib lowers 101 of 103, clib 21, posixlib 67, windowslib 34, javalib
   556 of 601, auxlib 7 of 15.
8. **Startup** (#9): decoded definitions and lowered code in the heap snapshot.

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
