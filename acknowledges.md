# Acknowledgements

Projects, papers and posts whose code or ideas this project took. Surveys of everything that
was compared are in `docs/research/`; this list covers what we actually use or plan to use.

## Code we ship

| Source | What we took | Licence |
|---|---|---|
| [Scala Native](https://github.com/scala-native/scala-native) 0.5.12 | The C runtime in `native/` (nativelib, javalib, posixlib, clib), including the Immix collector (`gc/immix`, `gc/immix_commix`, `gc/shared`, with one patch in `Marker.c`); in `overlay/src`, javalib's `LinkedTransferQueue.remove` (a JSR-166 port) and `ForkJoinPool.helpAsyncBlocker`, patched | BSD-3, `native/LICENSE-scala-native.md`; JSR-166 public domain |
| [Scala 3 compiler (dotty)](https://github.com/scala/scala3) 3.9.0 | `crates/syntax/src/scan`: a Rust translation of `parsing/Scanners.scala`, `Tokens.scala`, `CharArrayReader.scala` and `util/Chars.scala`; `crates/syntax/src/parse` and `ast`: of `parsing/Parsers.scala`, `ast/untpd.scala`, `ast/Trees.scala`, `ast/Positioned.scala`, `util/Spans.scala`, `core/Flags.scala` and the parser's messages; `crates/syntax/src/parse/xml`: of `parsing/xml/MarkupParsers.scala`, `MarkupParserCommon.scala`, `SymbolicXMLBuilder.scala` and `Utility.scala`; `crates/syntax/src/desugar`: of `ast/Desugar.scala`, `ast/DesugarEnums.scala` and the tree copier of `ast/Trees.scala`; `crates/typer`: of `typer/Typer.scala`, `typer/Namer.scala`, `ast/MainProxies.scala`, the flag pickling of `core/tasty/TreePickler.scala` and `SymDenotations.fullName`; `crates/lower/src/phases`: of `transform/SyntheticMembers.scala` (`serializableObjectMethod`), the boxing of `transform/Erasure.scala`, the constant folding of `transform/FirstTransform.scala`, `transform/Getters.scala`, `transform/Memoize.scala`, the result-type bridges of `transform/Bridges.scala`, the `!=` rewrite of `transform/InterceptedMethods.scala`, the repeated arguments of `transform/ElimRepeated.scala`, `transform/SeqLiterals.scala` and `tpd.wrapArrayMethodName`, and the superclass call, trait constructors and forwarders of `Mixin.scala` and constructor body of `Constructors.scala` | Apache-2.0 |
| [Scala Native](https://github.com/scala-native/scala-native) 0.5.12 compiler and NIR | `crates/lower/src/codegen`: a Rust translation of nscplugin's `NirGenStat`, `NirGenExpr`, `NirGenName`, `NirGenType` and `NirCodeGen` (without native interop), of `ControlFlow.removeDeadBlocks` and of `Val.isZero` and `Val.isCanonical`; `crates/nir/src/encode` and `sig.rs`: of `serialization/BinarySerializer.scala`, `Sig.scala` and `Mangle.scala` | BSD-3 / Apache-2.0 |
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
- **Scala 3 compiler (dotty)** ([scala/scala3](https://github.com/scala/scala3)):
  `TastyFormat.scala`, `TastyReader.scala` and `TreeUnpickler.scala` for the TASTy reader
  (`crates/tasty`: its tag table and tree grammar); `Scanners`/`Parsers`/`Desugar` as the parser
  to port in phase 3; `TypeErasure.scala` and `transform/Bridges.scala` for erasure and bridges;
  `core/TypeComparer.scala` (`recur`, `firstTry`…`fourthTry`, `isSubArgs`, `either`) and the
  classes and the members of `Any` and `Object` that `Definitions` makes without TASTy, for subtyping in `crates/tasty/src/subtype`.
- **Scala Native's compiler plugin** (`nscplugin`): `NirGenStat.genInterfaceMethodBridge`, the
  bridges it adds for methods implementing a trait's abstract method; `NirGenName`, the JDK
  classes it renames and the `__` of `_Object`'s methods (the table in `crates/tasty`'s `jdk.rs`).

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
