# Acknowledgements

Projects, papers and posts whose code or ideas this project took. Surveys of everything that
was compared are in `docs/research/`; this list covers what we actually use or plan to use.

## Code we ship

| Source | What we took | Licence |
|---|---|---|
| [Scala Native](https://github.com/scala-native/scala-native) 0.5.12 | The C runtime in `native/` (nativelib, javalib, posixlib, clib), including the Immix collector (`gc/immix`, `gc/immix_commix`, `gc/shared`, with one patch in `Marker.c`) | BSD-3, `native/LICENSE-scala-native.md` |
| [Scala 3 compiler (dotty)](https://github.com/scala/scala3) 3.9.0 | `crates/syntax/src/scan`: a Rust translation of `parsing/Scanners.scala`, `Tokens.scala`, `CharArrayReader.scala` and `util/Chars.scala`; `crates/syntax/src/parse` and `ast`: of `parsing/Parsers.scala`, `ast/untpd.scala`, `ast/Trees.scala`, `ast/Positioned.scala`, `util/Spans.scala`, `core/Flags.scala` and the parser's messages | Apache-2.0 |
| Scala Native precompiled libraries | scalalib, javalib, nativelib, auxlib, posixlib and clib as NIR, executed by the VM (fetched, not vendored) | BSD-3 / Apache-2.0 |

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
- **HotSpot's tiered compilation and guarded inlining** (profile-driven inlining of
  monomorphic call sites behind a class check): our tier-up
  ([ADR 0006](docs/adr/0006-tier-up-inlining.md)).
- **scalino** ([coreyoconnor](https://github.com/coreyoconnor/scalino),
  [lolgab](https://github.com/lolgab/scalino)): evidence that dotc runs without a JVM and that
  macros can be interpreted from TASTy; its patched `quoted/Interpreter` is the reference for
  phase 4.
- **tasty-query** ([scalacenter/tasty-query](https://github.com/scalacenter/tasty-query)): the
  model of symbols and types loaded lazily from TASTy (phase 3).
- **Scala 3 compiler (dotty)** ([scala/scala3](https://github.com/scala/scala3)):
  `TastyFormat.scala` for the TASTy reader spike; `Scanners`/`Parsers`/`Desugar` as the parser to
  port in phase 3.

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
- Rust style: Reactive Core's `rcui` project (`CLAUDE.md`).
