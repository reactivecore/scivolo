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

- **Heap snapshot** (#9). Pre-initialize the stdlib modules and serialize them into the
  library image, the way Dart AppJIT, Julia's sysimage and GraalVM's image heap do.
- **Precise roots.** Typed slot maps would make frame scanning precise (ADR 0004); MMTk stays
  the alternative collector.
- **Optional JIT.** A Cranelift baseline tier for hot methods.

## Phase 3 — own Rust frontend: no JVM, no compile step

Exit criterion (met, 2026-10-15): editing and then running `moderate.scala` takes 48 ms with no
JVM (`just bench-edit`). Measured by dotty's `tests/run` through typer, lowering and VM
(`just dotty-lower-report`): 1,169 of 1,709 pass (2026-10-21); 300 stop at the typer, 18 at
the lowering.

1. **Parser** (`crates/syntax`): ports of dotty's `Scanners`, `Parsers`, `Desugar` and
   `DesugarEnums`. Tokens match dotty on all 5,893 files, trees and desugaring on 5,891
   (`just syntax-report`). Open: `-Wconf`.
2. **Symbols from TASTy** (`crates/tasty`, [research 08](docs/research/08-tasty-symbols.md)):
   lazily loaded symbols and types of the standard library and javalib, subtyping, erasure,
   bridges; 99.8% of the library's signatures match Scala Native's mangled names.
3. **Typer for the scripting subset** (`crates/typer`): inference, overloads, implicits and
   givens, extension methods, unions, inlining (ADR 0020), match types, mirrors and `derives`.
   `just typer-report`: tests/pos 1,359 typed as dotty, 32 differ, 1,618 refused. Unsupported
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
`Quotes` is refused by name. Circe 0.14.10's `derives Codec.AsObject` works. dotty's
`tests/run-macros`: 7 of 321 pass (`just dotty-macros`).

## Phase 5 — precompiled code and wider reach

- **Native executables** (done, #113, [ADR 0028](docs/adr/0028-native-executables-via-cranelift.md)):
  `scivolo-aot` builds sources or NIR into single-threaded executables with Cranelift; 144 of 150
  test programs from source (`just aot-src-all`).
- **Third-party libraries** (done): `--dep` loads `_native0.5_3` jars as NIR plus TASTy, fetched
  by Coursier.
- **AOT for libraries.** Compile hot stdlib NIR ahead of time, callable from interpreted code.
- **JVM bytecode frontend (optional).** Needs `invokedynamic`/LambdaMetafactory and JDK
  natives, and runs into the JDK's licence. Low priority.

## Next

In order of the tests they unblock:
1. **Typer groups**: [research 27](docs/research/27-typer-groups-after-23.md)'s I (inline
   leftovers), V2 (applications) and U (structural and small) and V1's singles (tuples, i4364a,
   i19641, i25000); [research 28](docs/research/28-adaptation-members-implicits.md)'s A
   leftovers (extension fallback, i16065), S's `Manifest` and P (search singles); named-tuple
   leftovers (#139: patterns over 22, `map[F]`, ordering; #146); poly function leftovers (#136); mirrors (#129); match-type leftovers (#120);
   typed-tree printing gaps (#122, `RefinedTypeTree`).
2. **Macros** (#40, [research 26](docs/research/26-macros-next.md)): `Symbol.flags`, term
   extractors and `Varargs`, `'[t]` patterns, `Type.show`, quote matcher, reflect slices,
   measured on `tests/run-macros`.
3. **Startup** (#9): the VM's `run_main` from a heap snapshot.
4. **Lowering gaps** (research 15): reflective calls, classes in local classes capturing values,
   traits nested in local traits; classes extending `FunctionN` above 22
   (`FunctionXXLForwarders`), context results needing `Bridges` (implicitFuns), the rest of
   `TupleOptimizations` (#138), JDK forwarders (#135).
5. **Inlining leftovers** (research 12): `letBindUnless` projections, inline accessors of
   polymorphic methods, outer `this` through `outerSelect`, inline methods of earlier REPL
   inputs; the forward-reference check (#90).
6. **Value classes** (#88): ExpandSAMs closures (i24573), the remaining corpus as the typer
   catches up.
7. **Tooling:** test programs compiled with Scala 3.9.0 explicitly (#91).

Process notes: each batch is reviewed by a subagent before its MR; `just snapshot-diff programs
lower` shows worsened tests, which needs `tmp/dotty/3.9.0-0.5.12/results.tsv` from `just
dotty-build` and `just dotty-report` in every checkout that runs it.

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
