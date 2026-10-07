# Roadmap — a fast Scala 3.9 interpreter in Rust

Goal: run Scala 3.9.0 scripts with no JVM, **≤ 50 ms end to end after an
edit** (stretch goal: 20 ms, which is python3's startup). Coverage should
equal the Scala Native subset and be proven with the suites Scala Native
passes. Precompiled libraries must be usable. JVM bytecode is optional.

Evidence for every number below is in `docs/research/` and `spikes/*/README.md`
(see `README.md`). The architectural decision is
[ADR 0001](docs/adr/0001-nir-shaped-execution-ir.md).

## Architecture in one picture

```
 script.scala ──► [frontend] ──► NIR-shaped IR ──► [VM: closure-compiled SSA] ──► result
                    │   ▲                              │
     phase 1: dotc+nscplugin (JVM or scalino),        ├─ library image: stdlib/javalib/nativelib NIR
              cached by content hash                  │   (Scala Native 0.5.12, precompiled, lazily loaded)
     phase 3: own Rust frontend ──── types from ──► library TASTy (scala-library-3.9.0)
                                                      └─ C runtime of Scala Native via FFI (GC, externs)
```

## Phase 0 — research and spikes (done, 2026-09-24)

- [x] Baseline latency (`spikes/00-baseline`). scala-cli with warm Bloop takes
      290–520 ms, 1–2 s without it. Scala Native binaries start in 4 ms but
      take 3.4–5.6 s to build.
- [x] TASTy reader in Rust (`spikes/01`): 941/941 stdlib files decode in
      12.7 ms; header, names and section index alone take 2.3 ms.
- [x] Parser evaluation (`spikes/02`): tree-sitter-scala misparses some input
      silently. Port dotty's parser instead (2.5–3.5 person-months).
- [x] Native dotc via scalino (`spikes/03`): source to TASTy takes 60–198 ms
      and macros mostly work. Usable as a JVM-free fallback, too slow for the
      primary path.
- [x] NIR reader in Rust (`spikes/04`): 9,677 library files decode. On
      `moderate.scala` the output matches Scala Native's own printer byte for
      byte.
- [~] Minimal TASTy evaluator (`spikes/05`): the loader works (hello takes
      3.9 ms per process). The evaluator is unfinished. It was superseded by
      ADR 0001 but documents what TASTy lacks for execution.

## Phase 1 — NIR VM: "run what Scala Native runs" (~4–6 person-months)

Progress (2026-09-25): workspace and `crates/nir` done; `crates/vm` interprets lowered NIR
with Scala Native's C runtime built from `native/` (`crates/snrt`). **M1, M2 and M3 pass**:
11,336 of Scala Native's unit tests pass, 3 fail, 21 skip ([docs/conformance.md](docs/conformance.md)).
Memory is collected by Scala Native's Immix ([ADR 0004](docs/adr/0004-immix-garbage-collector.md), done).
Stack traces come from the interpreter's frames, and Java threads run as green threads
([ADR 0003](docs/adr/0003-green-threads-first.md), done 2026-09-27).
M4 (2026-09-28): dotty's `tests/run` fail in the VM only where they fail in Scala Native too
([docs/conformance.md](docs/conformance.md)). M5: every scala/scala JUnit test that compiles with
Scala 3 passes.

Next steps in phase 1:
1. ~~Garbage collector~~ (done 2026-09-25, ADR 0004).
2. ~~Green threads~~ (done 2026-09-27, ADR 0003).
3. OS threads (later): a concurrency-safe heap, caches and callback table.
4. ~~Linux x86-64: CI build and test run; variadic calls with floating-point arguments~~
   (done 2026-09-28, #1).

Execute NIR from any source with Scala Native's precompiled libraries. The
frontend is still dotc + nscplugin, behind a cache.

1. **Workspace.** A cargo workspace with rcui-style crates and a `Justfile`
   (`just gates`). Planned crates:
   - `nir`: spike 04 hardened, with `show/op.rs` split
   - `image`: the library index
   - `vm`
   - `runtime`: heap, GC, FFI
   - `cli`
2. **Library image.** A prebuilt, uncompressed, mmap-able index of every NIR
   global (name → file and offset). Methods decode on first call. Target
   under 5 ms to open. Today a full decode costs about 190 ms and unzipping
   about 130 ms.
3. **VM core.** Each method is lowered on first call to closure-compiled SSA
   with register slots. Semantics follow Scala Native's `Lower.scala`:
   - null checks, bounds checks and division checks
   - `is`/`as` type tests
   - `unwind` edges for exceptions
   - module initialization
   - vtable and itable dispatch, built from class and trait definitions
4. **Runtime.** Scala Native's object layout: an RTTI header, UTF-16 strings,
   and its array layout. The Immix collector comes with Scala Native's own C
   runtime (ADR 0004). The ~534 extern symbols javalib calls go
   through `libffi` and `dlsym`. Pointer intrinsics (`Ptr`, `stackalloc`,
   `Intrinsics.*`) run on a byte-addressable native heap. Single-threaded
   first.
5. **Frontend shim.** A `scalai run script.scala` command:
   1. hash the script
   2. on a cache miss, compile it to NIR with dotc + nscplugin (a Bloop or
      scalino server)
   3. run the cached NIR

   The miss path costs 0.2–0.5 s. A hit costs VM time only.

**Conformance milestones**, taken from Scala Native's CI and dotty. The test
code is already NIR, so no frontend of our own is needed:

| M | Suite | Size |
|---|---|---|
| M1 | hello, `moderate.scala` and `cpu.scala` from `spikes/00-baseline` | 3 |
| M2 | SN `unit-tests/shared` language tests | 172 @Test |
| M3 | SN javalib tests, package by package (`java.lang` first) | 5,479 @Test |
| M4 | dotty `tests/run`, filtered to the SN subset, diffed against `.check` files | ~1,800 programs |
| M5 | `scala-partest-junit-tests` (scala/scala `test/junit`) | ~2,700 @Test |

The JUnit driver in Rust calls the generated `Bootstrapper`s directly.

**Exit criteria.** M2 passes and M3 is above 80%. A cached hello takes
≤ 20 ms. `cpu.scala` runs within 3× of CPython (113 ms).
Status (2026-09-27): M2 and M3 met; hello takes 20 ms (library image, ADR 0005); `cpu`
takes 0.34 s after tier-up inlining (ADR 0006), at the target. Phase 1 exit criteria met,
on the M4 Pro reference machine.

## Phase 2 — startup and speed (~3–4 person-months, overlaps phase 1)

Measurements and the ranked list of speed ideas: [docs/performance.md](docs/performance.md).

- **Heap snapshot.** Pre-initialize the stdlib modules (Predef, collections)
  and serialize them into the library image, the way Dart AppJIT, Julia's
  sysimage and GraalVM's image heap do.
- **Interpreter speedups.** Inline caches on `method` dispatch, specialization
  of operations by type, and super-instructions.
- **Precise roots.** Immix scans frames and the native stack conservatively
  (ADR 0004); typed slot maps would make frame scanning precise. MMTk stays the
  alternative collector.
- **Optional JIT.** A Cranelift baseline tier for hot methods. The Winch
  numbers suggest it compiles 15–20× faster at a cost of 1.1–1.5× in code
  quality.

## Phase 3 — own Rust frontend: no JVM, no compile step (~9–15 person-months)

1. **Parser.** Port dotty's `Scanners` and `Parsers`, about 7k lines of Scala
   becoming 8–10k lines of Rust. Also port `Desugar`. Test against dotty's
   parser on `tests/pos` and `tests/neg`
   ([research 06](docs/research/06-parser-port.md)). The scanner is done
   (`crates/syntax`, 2026-09-28): its tokens match dotty's on all 5893 files
   (`just syntax-report tokens`). The parser's trees and diagnostics match on
   5891 of them (`just syntax-report trees`); open: `-Wconf`, and a warning
   that quotes code through dotty's tree printer. The desugaring before
   typing (`Desugar`, `DesugarEnums`, [research 07](docs/research/07-desugar-port.md))
   matches dotty on the same 5891 files (`just syntax-report desugar`); the
   parts that need types wait for the typer.
2. **Symbols from TASTy.** Extend spike 01 into a model of symbols and types
   in the style of tasty-query, loaded lazily from an uncompressed cache of
   library TASTy ([research 08](docs/research/08-tasty-symbols.md), #28).
   Done: `crates/tasty` (2026-09-30): packages, lazily loaded files, symbols, declared
   types, parents, linearization, inherited members, base types, `asSeenFrom`,
   override matching, erasure, bridges; the JDK API from javalib's TASTy
   (`just javalib-tasty`). Of the method signatures and bridges of the whole
   standard library (15,587) and javalib (11,189), 99.8% and 99.9% match the
   mangled names in Scala Native's NIR. Subtyping with type variables behind
   the typer's constraint agrees with dotc on 13,456 pairs; `lub`/`glb`,
   member lookup on types, `NoInits` and `inline` bodies are there for the
   typer. The TASTy is read from the library image (ADR 0005): opening it
   takes about 1 ms.
3. **Typer for the scripting subset.** Cover:
   - local type inference
   - overload resolution
   - givens, `using` and extension methods
   - implicit conversions
   - union and intersection types, as far as scripts need them
   - expansion of `inline` defs from TASTy bodies

   Leave out match types, GADTs and capture checking until they are needed.
   Anything unsupported falls back to the phase 1 path, never silently.
4. **Lowering.** Pattern matching, lazy vals, closures, mixins, bridges and
   erasure, into the VM IR. The result must mangle names exactly as
   nscplugin does (ADR 0001).
5. **Differential testing.** Run M2–M5 twice, through the own frontend and
   through dotc, and compare the results.
6. **REPL.** Starts as soon as the own frontend runs simple programs. As in
   dotty's REPL, each input becomes an object `rs$lineN` whose definitions
   later inputs import; it is typed against the earlier ones, lowered, and
   its NIR added to the running VM. An expression's value is printed with
   its type (`val res0: Int = 3`). Input the frontend cannot handle yet is
   reported as unsupported, never run through another path (#37). Runs as
   `scalai` without a target (2026-10-07): values (printed as dotty's
   pprint), defs, type aliases, imports, lambdas, shadowing, `:reset`,
   `:type`, stack traces with causes and suppressed exceptions and the
   `res` numbering match dotty 3.9 on the transcripts in
   `tests/repl` (`just repl-oracle`), but for error messages; Ctrl-C stops
   an input. Classes, case classes, traits and objects defined in one input
   are used, extended, matched on and redefined in later ones, with default
   and named arguments (`tests/repl/classes.check`). Lazy vals, enums,
   generic case classes and mutable class parameters wait for the lowering;
   some adaptations and overloads, compound assignment to a selection,
   defaults needing lifting, and dotty's messages for unresolved names, for
   the typer.

**Exit criterion.** Editing and then running `moderate.scala` takes ≤ 50 ms
with no JVM involved.

## Phase 4 — macros without a JVM (~4–6 person-months)

- Our own implementation of `scala.quoted.Quotes` in Rust, on top of the
  phase 3 trees and types. Macro implementations are ordinary library code
  that the VM already runs from NIR.
- scalino's patched `quoted/Interpreter` is the reference design; its gap is
  quote type patterns (`case '[T]`).
- Until this phase lands, scripts that call macros take the phase 1 fallback.

## Phase 5 — precompiled code and wider reach (open-ended)

- **AOT for libraries.** Compile library NIR through LLVM (Scala Native's own
  backend) or Cranelift. The result must use the VM's object layout and
  calling convention, so interpreted and native code can call each other.
  Priority: hot stdlib paths first.
- **Third-party libraries.** Load `_native0.5_3` jars (e.g. cats-core) as NIR
  plus TASTy, resolved like scala-cli `//> using dep` directives.
- **JVM bytecode frontend (optional).** Map classfiles to the same IR. This
  needs `invokedynamic`/LambdaMetafactory and JDK natives, and runs into the
  JDK's licence. Low priority.

## Next concrete steps (first 2–4 weeks)

1. Set up the workspace and `Justfile`. Promote the spike 04 reader to
   `crates/nir`: split `show/op.rs`, add tests against Scala Native's printer.
2. **Spike: mangling round trip.** Take a TASTy method symbol, compute its
   erased signature, apply Scala Native's mangling and look it up in the NIR
   index. This is the key risk in ADR 0001.
3. **Spike: link Scala Native's C runtime** (nativelib `.c`/`.cpp` with Boehm)
   into a Rust binary and call `scalanative_*` from Rust.
4. Build the library index file and measure its open time against the 5 ms
   target.
5. Get the VM running M1 (hello) end to end.

## Decisions (2026-09-25)

1. **ADR 0001 accepted:** code from NIR, types from TASTy. NIR gives an exercised
   environment (Scala Native's libraries and test suites) to start from.
2. **Compiler until the own frontend exists:** scala-cli (JVM, Bloop) for development and
   tests. scalino is not used.
3. **Scala Native coupling:** stay on the 0.5.x NIR format and port to 0.6 when it ships.
4. **Threads ([ADR 0003](docs/adr/0003-green-threads-first.md)):** green threads first,
   also backing Java virtual threads; real OS threads later.
5. **Platforms:** macOS arm64 and Linux x86-64 are the targets. Development happens on
   Apple Silicon; GitLab CI tests both on every MR and on main.

## Risks

- **Version churn.** The NIR format changes with Scala Native 0.6. TASTy
  changes with every Scala minor release, but 3.9 is an LTS.
- **javalib's C side.** Its externs and pointer code need a faithful native
  memory model. Only 0.6% of javalib methods and 11% of nativelib methods use
  raw memory, but the core runtime depends on them.
- **Typer scope creep in phase 3.** The fallback path has to stay cheap so
  that partial coverage is acceptable.
- **Multithreading.** Scala Native's safepoints and monitors come later. Until
  then, scripts that use threads are out of scope for phase 1.
