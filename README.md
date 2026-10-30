# Scivolo

A Scala 3.9 interpreter and native compiler for scripts, written in Rust, with no JVM.

- Runs `.scala` and `.sc` scripts, a REPL, and builds native executables.
- Executes Scala Native's NIR with Scala Native's precompiled libraries and C runtime.
- Has its own frontend: dotty's parser, typer and lowering phases, ported to Rust.

## Status of the project

- **Research and benchmarking project**, not a product. No releases, no stability guarantees;
  not for production use. Build it from source.
- **Written by AI agents** (Claude Code): an experiment in how far that approach carries a
  project of this size.
  - Humans chose the goals and the order of work, reviewed code style and tested behaviour.
  - Most code has not been reviewed line by line; it has not been audited for security.
  - Correctness rests on automated checks against upstream: the NIR the frontend writes is
    compared byte for byte with Scala Native's, and the VM runs Scala Native's and dotty's test
    suites (see Progress).
- **The prior art belongs to the Scala and Scala Native teams.** Most of the code is a
  mechanical translation of the Scala 3 compiler (dotty) and of Scala Native's compiler plugin
  and runtime. Their designs and algorithms are kept; Scivolo changes the language and the
  execution model.
- **How translated code is marked:**
  - each file with translated code starts with a license notice naming its upstream files
    (`// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:`);
  - patched Scala Native C files start with `// Changed for Scivolo` and mark their lines;
  - [NOTICE](NOTICE) carries the upstream attributions, [acknowledges.md](acknowledges.md)
    the projects, designs and ideas used.

## Why

- We want scripts that start fast, use little memory and are type-checked.
- We love Scala and want it as a scripting language.
- We want to find out how far an AI-driven port of a compiler and runtime carries.

## Goals

Simplified from [ROADMAP.md](ROADMAP.md), which holds the targets and their status.

1. **Fast start:** edit-then-run of a small script ≤ 50 ms, about 3x python3's startup;
   CPU-bound code within 3x of CPython.
2. **Byte-exact translation and interpretation:** the own frontend writes the same NIR as
   Scala Native's compiler, byte for byte; the VM prints what Scala Native binaries print.
3. **Fast compilation:** no JVM, no compile server.
4. **Macros without a JVM:** library macros run from their NIR in the VM.

## Progress

Headline numbers (macOS arm64, M4 Pro); every suite, date and gap is in
[docs/conformance.md](docs/conformance.md) and [ROADMAP.md](ROADMAP.md).

- **VM:** 11,336 of Scala Native's 11,360 unit tests pass (NIR compiled by Scala Native).
- **Own frontend:** 1,441 of dotty's 1,711 `tests/run` pass through parser, typer, lowering
  and VM.
- **Macros:** 273 of dotty's 321 `tests/run-macros` pass.
- **Scala Native's library through the own frontend:** javalib 594 of 601 files, auxlib 15 of
  15, nativelib 102 of 103, clib, posixlib and windowslib complete.
- **Native executables:** 218 of 226 test programs, built from source.
- **Startup:** hello 7.5 ms from the heap snapshot; `moderate.scala` 48 ms edit-then-run
  (scala-cli: 360–520 ms, python3 startup: 18 ms).

Features:

- [x] NIR interpreter with Scala Native's libraries, Immix GC, exceptions with stack traces
- [x] Java threads as green threads
- [x] Own frontend for the scripting subset; scala-cli as the fallback for the rest
- [x] Scripts with `//> using` directives, dependencies from Maven via Coursier
- [x] REPL with completion, history and dependencies
- [x] Native executables via Cranelift (single-threaded)
- [x] Macros: a subset of `scala.quoted` (Circe's `derives Codec.AsObject` works)
- [x] Startup from a heap snapshot
- [ ] Typer: the rest of Scala 3 (match types in part; GADTs, capture checking)
- [ ] Macros: the rest of `Quotes`
- [ ] OS threads
- [ ] JIT compiler

## Platforms

- macOS arm64 (development)
- Linux x86-64
- Both tested in GitLab CI. Windows is not supported.

## Usage

Needs Rust 1.97+, `just`, a C compiler; optional: Coursier (`--dep`), scala-cli (fallback
frontend).

```sh
just fetch-libs                 # once: Scala Native's library jars into ref/libs
cargo build --release -p scivolo -p scivolo-aot
```

Outside `just`, set `SCIVOLO_LIBS` to `ref/libs`, or let `scivolo` find the jars in the
Coursier cache.

### Scripts

```sh
scivolo hello.scala ARGS        # compile through the own frontend (cached), then run
scivolo --main Main app.scala   # choose the main class
scivolo -O -Werror app.scala    # pass a Scala compiler option
scivolo --dep io.circe::circe-parser::0.14.10 app.scala
```

- Also runs as a `#!/usr/bin/env scivolo` shebang.
- The compiled NIR is cached; later runs compile nothing.
- Code the own frontend refuses compiles through scala-cli, with a note on stderr.
- Options, frontends, cache and diagnostics: [docs/scripts.md](docs/scripts.md).

### REPL

```sh
scivolo                         # or: just repl
scivolo --dep io.circe::circe-generic::0.14.10
```

```
Welcome to Scivolo 0.1.4 (target Scala 3.9.0). Type in expressions for evaluation. Or try :help.
> case class P(x: Int)
// defined case class P
> P(1)
val res0: P = P(1)
```

### Native executables

```sh
scivolo-aot -o hello hello.scala       # from sources, through the own frontend
scivolo-aot --cp DIR -o app Main       # from NIR files in DIR, main class Main
./hello
```

## How it works

```mermaid
flowchart LR
  src[script.scala] --> syntax[syntax: scanner, parser, desugar]
  syntax --> typer[typer: types, implicits, inlining]
  typer -->|macro calls| macros[macros: run in the VM]
  macros --> typer
  typer --> lower[lower: dotty's phases, NIR generation]
  lower --> nir[(NIR files)]
  libs[(Scala Native libraries: NIR + TASTy)] --> image[library image]
  image -->|TASTy| typer
  image -->|NIR| vm
  nir --> vm[vm: interpreter] --> out[output]
  nir --> aot[aot: Cranelift] --> exe[native executable]
  libs -->|NIR| aot
  rt[snrt: Scala Native's C runtime] --> vm
  rt --> exe
```

1. **Parse:** dotty's `Scanners`, `Parsers` and `Desugar` produce untyped trees.
2. **Type:** dotty's typing algorithms for the scripting subset, with symbols and types read
   lazily from the libraries' TASTy. Inlining and macro expansion run inside the typer.
3. **Lower:** dotty's phases after the typer, in dotty's order: FirstTransform, ExpandSAMs,
   ProtectedAccessors, ExtensionMethods, Getters, PatternMatcher, CapturedVars, ElimOpaque,
   ExplicitOuter, Erasure, LambdaLift, ElimRepeated, ResolveSuper, TailRec, Mixin, LazyVals,
   Memoize, Constructors (`crates/lower/src/phases/pipeline.rs` has the full order). Then
   Scala Native's NIR generation (nscplugin's `GenNIR`).
4. **Interpret:** the VM lowers each method on its first call to closure-compiled SSA. Objects
   use Scala Native's layout; the C runtime and Immix GC come from Scala Native, called through
   FFI. Library NIR is read lazily from a memory-mapped library image.
5. **Compile:** `scivolo-aot` translates the reachable NIR to Cranelift IR and links it with
   Scala Native's C runtime into an executable.

Decisions are in [docs/adr/](docs/adr/), research reports in [docs/research/](docs/research/).

## Crates

The package in `crates/<dir>` is `scivolo-<dir>` (`crates/scivolo` is `scivolo`); a library
keeps the short name (`use vm::…`).

| Dir | Purpose |
|---|---|
| `scivolo` | The CLI: runs scripts and NIR programs, starts the REPL, caches compiled scripts. |
| `aot` | `scivolo-aot`: compiles sources or NIR into a native executable with Cranelift. |
| `syntax` | Port of dotty's scanner, parser and desugaring: untyped trees. |
| `typer` | Port of dotty's typer for the scripting subset: typed trees, implicits, inlining. |
| `tasty` | Reads TASTy files; models the libraries' symbols and types, subtyping and erasure. |
| `lower` | Ports of dotty's post-typer phases and of Scala Native's NIR generation. |
| `macros` | Runs macro implementations in the VM and unpickles their quotes into typed trees. |
| `repl` | A REPL after dotty's: each input becomes a wrapper object run in one VM. |
| `vm` | The NIR interpreter: lowering to closures, heap, dispatch, green threads, FFI. |
| `nir` | Reader and writer of Scala Native 0.5 NIR. |
| `libs` | Locates the library jars (`SCIVOLO_LIBS` or the Coursier cache) and fetches dependencies. |
| `libimage` | The memory-mapped library image of NIR and TASTy, and the upkeep of its cache. |
| `snrt` | Builds Scala Native's C runtime from `native/`: a shared library and a static archive. |
| `nir-dump` | Prints NIR files as Scala Native's printer does; a development tool. |

Outside the crates:

- `overlay/src`: Scala sources that replace or add library members (javalib fixes, the REPL's
  value printer), compiled to `overlay/nir` ([ADR 0009](docs/adr/0009-library-overlay.md)).
- `native/`: Scala Native's C runtime sources.
- `tests/`: test programs, REPL transcripts, AOT programs; `spikes/`: throwaway experiments.

## Checked-in upstream artifacts

| Path | What | Rebuilt by |
|---|---|---|
| `native/` | Scala Native 0.5.12's C runtime (nativelib, javalib, posixlib, clib), copied; patched files marked | by hand, checked by `just native-stale` |
| `libs/javalib-tasty-0.5.12.jar` | TASTy of Scala Native's javalib, which the published jar lacks | `just javalib-tasty` |
| `libs/jdk-stubs.jar` | TASTy of the JDK annotations javalib's sources use (`libs/jdk-stubs`), for `just lib-compile` | `just jdk-stubs` |
| `tests/programs/*/` | NIR and `.check` output of each test program, compiled by Scala Native 0.5.12: the byte oracle | `just program-update NAME` |
| `overlay/nir` | NIR of `overlay/src`, compiled by Scala Native's compiler | `just overlay` |
| `tests/macros/lib/macros.jar`, `crates/*/tests/**` jars, `.nir`, `.tasty` | Test fixtures compiled by dotty or Scala Native | `just macro-fixture`, `just inline-fixture` |

The library jars themselves are not checked in: `just fetch-libs` downloads them from Maven
Central and checks their SHA-256.

## Development

```sh
just gates                      # fmt, clippy, tests: must pass before every commit
just check                      # every test program against its .check file, with timings
just --list                     # all recipes: oracles, reports, benchmarks
```

Rules for contributors (human or agent): [CLAUDE.md](CLAUDE.md). Work is tracked in GitLab
issues.

## License

Apache-2.0, see [LICENSE](LICENSE) and [NOTICE](NOTICE). Translated code keeps its upstream
license notices; JLine (BSD-3-Clause) and PPrint (MIT) in [licenses/](licenses/).
