# Acknowledgements

Projects, papers and posts whose code or ideas this project took. Surveys of everything that
was compared are in `docs/research/`; this list covers what we actually use or plan to use.

## Code we ship

A file with code translated from one of these projects names its upstream files in its
license notice, the first lines of the file; [NOTICE](NOTICE) carries the projects'
attributions.

### [Scala 3 compiler (dotty)](https://github.com/scala/scala3) 3.9.0

Apache-2.0. The compiler, scala3-repl and tasty-core, translated to Rust in `crates/syntax`,
`crates/typer`, `crates/tasty`, `crates/lower`, `crates/repl`, `crates/macros` and
`crates/vm`; `overlay/src/scala/quoted/runtime/impl` follows `QuotesImpl`.

### [Scala Native](https://github.com/scala-native/scala-native) 0.5.12

Apache-2.0, `native/LICENSE-scala-native.md`; JSR-166 public domain.

- Runtime: `native/` holds the C runtime of nativelib, javalib, posixlib and clib with the
  Immix collector, copied as is; a changed file says so in its first line. `overlay/src/java`
  patches javalib classes.
- Compiler and NIR: nscplugin and `nir` translated to Rust in `crates/lower`, `crates/nir`,
  `crates/tasty` and `crates/vm`.
- Precompiled libraries: scalalib, javalib, nativelib, auxlib, posixlib and clib as NIR,
  executed by the VM (fetched, not vendored).

### [JLine](https://github.com/jline/jline3) 4.0.14

BSD-3-Clause, `licenses/jline.txt`. The REPL's history file and completion matchers, in
`crates/scivolo` and `crates/repl`.

### [PPrint](https://github.com/com-lihaoyi/PPrint) by Li Haoyi, as vendored by dotty 3.9.0

MIT, `licenses/pprint.txt`. The REPL's value printer in `overlay/src/scivolo/repl`, without
colors (fansi).

## Designs we follow

- **Scala Native NIR and its lowering** (`nir/`, `tools/.../codegen/Lower.scala`,
  `RuntimeTypeInformation.scala`, `Generate.scala`): the execution IR, the object and array
  layout, RTTI, name mangling, module initialization, and the linker symbols the runtime expects.
  See [ADR 0001](docs/adr/0001-nir-shaped-execution-ir.md). Its `ResourceEmbedder` gives the
  files left out of classpath resources ([ADR 0007](docs/adr/0007-classpath-resources-and-services.md)).
- **Immix**, S. M. Blackburn and K. S. McKinley, "Immix: A Mark-Region Garbage Collector with
  Space Efficiency, Fast Collection, and Mutator Performance", PLDI 2008; used through Scala
  Native's implementation ([ADR 0004](docs/adr/0004-immix-garbage-collector.md)).
- **sjsir-interpreter** by Sébastien Doeraene ([sjrd/sjsir-interpreter](https://github.com/sjrd/sjsir-interpreter)):
  compiling a typed Scala IR to closures/lowered instructions instead of walking trees.
- **Neil Mitchell, "Writing a fast interpreter"**
  ([blog](http://neilmitchell.blogspot.com/2020/04/writing-fast-interpreter.html)): closure
  compilation and resolving variables to slots ahead of execution.
- **CPython 3.11's specializing adaptive interpreter**
  ([What's new](https://docs.python.org/3/whatsnew/3.11.html)): per-site inline caches and
  specialization, behind our call-site cells and superinstructions
  ([docs/performance.md](docs/performance.md)).
- **GraalVM Native Image substitutions** (`@TargetClass`, `@Substitute`, `@Alias`): replacing
  library methods member by member from separately compiled classes, behind the library
  overlay ([ADR 0009](docs/adr/0009-library-overlay.md)).
- **HotSpot's tiered compilation and guarded inlining** (profile-driven inlining of
  monomorphic call sites behind a class check): our tier-up
  ([ADR 0006](docs/adr/0006-tier-up-inlining.md)).
- **scalino** ([coreyoconnor](https://github.com/coreyoconnor/scalino),
  [lolgab](https://github.com/lolgab/scalino)): evidence that dotc runs without a JVM and that
  macros can be interpreted from TASTy; its patched `quoted/Interpreter` is the reference for
  phase 4.
- **tasty-query** ([scalacenter/tasty-query](https://github.com/scalacenter/tasty-query)): the
  model of symbols and types loaded lazily from TASTy (phase 3).

- **Cranelift** ([cranelift.dev](https://cranelift.dev/)), Apache-2.0 with LLVM exception: the
  code generator of `scivolo-aot` (`crates/aot`), used as a library
  ([ADR 0028](docs/adr/0028-native-executables-via-cranelift.md)); Scala Native's `Lower.scala`
  and `Generate.scala` give the semantics it lowers NIR to.

- **Go's scheduler** hands a goroutine's blocking system call to another OS thread; our green
  threads do the same for `blocking` externs (ADR 0003). **corosensei**
  ([crate](https://docs.rs/corosensei)) provides the stackful coroutines.

## Ideas on the roadmap

- **Heap snapshots / prebuilt images:** Dart snapshots
  ([wiki](https://github.com/dart-lang/sdk/wiki/Snapshots)), Julia system images
  ([PackageCompiler.jl](https://julialang.github.io/PackageCompiler.jl/stable/sysimages.html)),
  the GraalVM Native Image heap, Emacs' portable dumper (`src/pdumper.h`).
- **Baseline JIT:** Cranelift ([cranelift.dev](https://cranelift.dev/)) and Wasmtime's Winch
  baseline compiler ([README](https://github.com/bytecodealliance/wasmtime/blob/main/winch/README.md)).
- **Alternative collectors:** MMTk ([mmtk-core](https://github.com/mmtk/mmtk-core)) and Boehm GC
  ([bdwgc](https://github.com/bdwgc/bdwgc)) as fallbacks named in ADR 0004.

## Tests and references

- Scala Native's unit tests (`unit-tests/`, JUnit runtime and test interface): our conformance
  suite ([docs/conformance.md](docs/conformance.md)).
- dotty `tests/run` and scala/scala `test/junit`: planned milestones M4 and M5.
- scala3-repl 3.9.0's `ReplDriver.completions` and `interactive/Completion`: the answers of
  `tests/repl-completion` and the algorithm to port (research 11), with
  [JLine](https://github.com/jline/jline3) 4.0.14's `CompletionMatcherImpl` (BSD-3), the word
  filter applied to them.
- Rust style: Reactive Core's `rcui` project (`CLAUDE.md`).
