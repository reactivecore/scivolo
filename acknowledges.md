# Acknowledgements

Projects, papers and posts whose code or ideas this project took. Surveys of everything that
was compared are in `docs/research/`; this list covers what we actually use or plan to use.

## Code we ship

### [Scala Native](https://github.com/scala-native/scala-native) 0.5.12 runtime

BSD-3, `native/LICENSE-scala-native.md`; JSR-166 public domain.

- `native/`: the C runtime of nativelib, javalib, posixlib and clib, with the Immix collector
  (`gc/immix*`, `gc/shared`; one patch in `Marker.c`).
- `overlay/src`: javalib's `LinkedTransferQueue.remove` (a JSR-166 port) and
  `ForkJoinPool.helpAsyncBlocker`, patched.

### [Scala 3 compiler (dotty)](https://github.com/scala/scala3) 3.9.0

Apache-2.0. Rust translations of:

- `crates/syntax/src/scan`: `parsing/Scanners`, `Tokens`, `CharArrayReader`, `util/Chars`.
- `crates/syntax/src/parse`, `ast`: `parsing/Parsers`, `ast/{untpd,Trees,Positioned}`,
  `util/Spans`, `core/Flags`, the parser's messages.
- `crates/syntax/src/parse/xml`: `parsing/xml/*` (`MarkupParsers`, `MarkupParserCommon`,
  `SymbolicXMLBuilder`, `Utility`).
- `crates/syntax/src/desugar`: `ast/Desugar`, `ast/DesugarEnums`, the tree copier of `ast/Trees`.
- `crates/typer`: `typer/{Typer,Namer}`, `typer/TypeAssigner.superType`, `ast/MainProxies`, the flag pickling of
  `core/tasty/TreePickler`, `SymDenotations.fullName`; `crates/typer/src/tpd/walk.rs`:
  `ast/Trees` `TreeTraverser`, `TreeAccumulator`, `DeepFolder`, `tpd.localCtx`.
- `crates/lower/src/phases`, from `transform/`:
  - `SyntheticMembers` (`serializableObjectMethod`, case class members, mirror support),
    `FirstTransform` (constant folding, `classOf`)
  - `Getters`, `Memoize` (`erasedBottomTree`), `Mixin` (superclass call, trait constructors,
    forwarders, `traitInits` of lazy values), `MixinOps` (`mkForwarderSym`),
    `Constructors` (constructor body, `intoConstr`, `markUsedPrivateSymbols`)
  - `BetaReduce`, `LetOverApply`, `NonLocalReturns`
  - `Erasure` (boxing, `adaptClosure`, `cast` of primitive arrays),
    `TypeErasure.additionalAdaptationNeeded`, `Bridges` (result-type bridges, inherited pairs),
    `core/Decorators.reduceBalanced`
  - `InterceptedMethods` (`!=`), `ElimRepeated` (`adaptToArray`, `arrayToSeq`), `SeqLiterals`,
    `tpd.wrapArrayMethodName`
  - `LambdaLift`, `Dependencies` (closures, local methods, returns of blocks), `CapturedVars`,
    `ElimByName` (calls)
  - `PatternMatcher` (literal, binding, alternative, type, extractor and sequence patterns,
    guards, switches),
    `TryCatchPatterns`, `TypeTestsCasts` (type tests of primitives, unions and intersections,
    casts of primitives, `primitiveConversion`),
    `ast/Positioned.envelope`, `ast/Trees.TreeCopier` (spans of the trees phases make or copy)
  - `SpecializeFunctions`, `SpecializeApplyMethods`
  - `ExpandPrivate`, `SymDenotations.ensureNotPrivate`, `expandedName`
  - `SuperAccessors` (`superAccessorCall`, `transformSuperSelect`), `ResolveSuper`
    (`rebindSuper`)
  - `localopt/StringInterpolatorOpt` (`s`, `raw`, `transformS`), `tpd.evalOnce`
  - `VCInlineMethods`, `ExtensionMethods` (extension methods)
  - `LazyVals` (`transformMemberDefThreadSafeNew`, `mkThreadSafeDef`, `transformLocalDef`,
    `transformStats`), `TypeErasure.isPolymorphicAfterErasure` (`synchronized`)
  - `TailRec` (`TailRecElimination`)
- `crates/tasty/src/erasure/scala2.rs`: `core/unpickleScala2/Scala2Erasure`
  (`intersectionDominator`), `SourceLanguage`.
- `crates/tasty/src/types/show_user`: `printing/{ReplPrinter,RefinedPrinter,PlainPrinter}`
  (types and `dclText` as the REPL shows them).
- `crates/repl`, from `repl/` (scala3-repl): `ReplPhase` (wrapping, `res` names),
  `ParseResult` (commands, help text, incomplete input), `Rendering` and `ReplDriver`
  (definitions shown), `ReplBytecodeAnalysis` (the synthesized `toString` test in
  `crates/vm/src/natives/repl.rs`); the transcript format of `compiler/test-resources/repl`;
  from the compiler, `reporting/MessageRendering` (diagnostic boxes) and the `ErrorMessageID`
  numbers and `MessageKind`s of `reporting/messages` (`crates/syntax/src/diag.rs`);
  `printing/SyntaxHighlighting` and the `nameSpan`/`endSpan` of `ast/Trees`
  (`crates/repl/src/highlight.rs`), with `JLineTerminal`'s magenta prompt.
- Completion: `interactive/Completion` (`completionPrefix`, `accessibleMembers`,
  `isValidCompletionSymbol`, `needsBacktick`) in `crates/typer/src/completion` and
  `crates/repl/src/completion`; `ast/NavigateAST.pathTo` (`crates/syntax/src/ast/path.rs`);
  scala3-repl's `JLineTerminal.Parser` (the word under the cursor).

### [JLine](https://github.com/jline/jline3) 4.0.14

BSD-3. `crates/scalai/src/history.rs`: `reader/impl/history/DefaultHistory` (file format,
`escape`/`unescape`, the add filters, `trimHistory`/`doTrimHistory`, the size limits).
`crates/repl/src/completion/listed.rs`: `CompletionMatcherImpl`'s default matchers,
`ReaderUtils.distance`, `utils/Levenshtein.distance` and the grouping and order of
`LineReaderImpl.computePost`.

### [PPrint](https://github.com/com-lihaoyi/PPrint) by Li Haoyi, as vendored by dotty 3.9.0

MIT. `overlay/src/scalai/repl`: a Scala port of `Walker`, `Renderer`, `Result`, `Truncated`,
`Util` and `ProductSupport`, without colors (fansi).

### [Scala Native](https://github.com/scala-native/scala-native) 0.5.12 compiler and NIR

BSD-3 / Apache-2.0. Rust translations of:

- `crates/lower/src/codegen`: nscplugin's `NirGen{Stat,Expr,Name,Type}` (`genTryFinally`, `genSynchronized` too), `NirCodeGen` (without
  native interop), `ControlFlow.removeDeadBlocks`, `Val.isZero`, `Val.isCanonical`,
  `AdaptLazyVals` (`objCAS` on a field pointer).
- `crates/lower/src/phases/enumerations.rs`: nscplugin's `PrepNativeInterop` (names of
  `Enumeration` values).
- `crates/nir/src/encode`, `sig.rs`: `serialization/BinarySerializer`, `Sig`, `Mangle`.

### Scala Native precompiled libraries

BSD-3 / Apache-2.0. scalalib, javalib, nativelib, auxlib, posixlib and clib as NIR, executed by
the VM (fetched, not vendored).

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
  `core/TypeComparer.scala` (`recur`, `firstTry`…`fourthTry`, `isSubArgs`, `either`,
  `compareAtoms`, `mergedLub`), `Atoms.scala`, `Type.atoms`, `hasClassSymbol`,
  `OrType.balanced` and `OrType.atoms` (its `myAtoms` cache), and the
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
- scala3-repl 3.9.0's `ReplDriver.completions` and `interactive/Completion`: the answers of
  `tests/repl-completion` and the algorithm to port (research 11), with
  [JLine](https://github.com/jline/jline3) 4.0.14's `CompletionMatcherImpl` (BSD-3), the word
  filter applied to them.
- Rust style: Reactive Core's `rcui` project (`CLAUDE.md`).
