# Scala 3 compiler pipeline, TASTy and macros: what an interpreter must re-implement

Research note for the Reactive Core "fast Scala 3 interpreter" project (Rust/C++ host, scripting focus, Scala-Native-compatible subset).
Date: 2026-09-24. Reference compiler: `scala/scala3` tag `3.9.0` (checked out locally and inspected; all line counts below are from that tag). Also inspected: `scalacenter/tasty-query` main (2026-09-08), and the published `org.scala-lang:scala-library:3.9.0` jar.

Legend used throughout:

- **[V]** verified directly from a primary source (compiler source at tag 3.9.0, published jar, official docs/release notes).
- **[I]** my inference / judgement, built on the verified facts.

---

## 0. TL;DR

1. **Scala 3.9.0 is released (3 Sept 2026) and is the new LTS** (3-year support, JDK 17+ baseline). Its TASTy version is **28.9-0**. [V]
2. **TASTy is emitted by the `Pickler` phase, which runs *before* `Inlining`.** So TASTy contains fully typed trees with resolved implicits and overloads, **but non-`transparent` `inline` calls (including most macro calls) are *not* expanded in TASTy**. Only `transparent inline` calls are expanded during `Typer` and therefore appear as `INLINED` nodes in TASTy. [V] A TASTy-based interpreter must therefore implement the inliner (which includes `summonInline`/`summonFrom` implicit search at the call site and `inline match`/match-type reduction) or restrict itself to code where these do not occur. [V+I]
3. **Macros are executed by the JVM compiler via reflection**: `transform.Splicer` → `quoted.Interpreter` (392 lines) interprets only the *call shape* of the splice (`Call`, literals, closures, blocks, `Inlined`, `Typed`, varargs) and then invokes the precompiled macro implementation class with `java.lang.reflect.Method.invoke` through a `MacroClassLoader` built from the classpath. The macro body itself is never interpreted. [V] Scala.js and Scala Native inherit this because their code-gen phases (`GenSJSIR`, `scalanative-genNIR`) sit at the *end* of the JVM-hosted pipeline. [V]
4. **The typer is ~25 kLOC (`typer/`) sitting on a ~46 kLOC `core/` (`Types.scala` 7.3k, `TypeComparer.scala` 4.1k), plus ~10 kLOC `cc/` for the still-experimental capture checker.** A faithful reimplementation is a multi-person-year effort; reimplementing "just enough" typer to inline and to check nothing is more tractable but still needs subtyping, `asSeenFrom`, member lookup and implicit search. [V for sizes, I for effort]
5. **Since Scala 3.8 the standard library is compiled by Scala 3**, and `org.scala-lang:scala-library:3.9.0` ships **941 `.tasty` files** next to 3662 `.class` files (`scala3-library_3:3.9.0` is now a 344-byte shim depending on it). So the whole stdlib is available as TASTy 28.9 for interpretation or precompilation. The stdlib uses `inline def` in only 9 source files (63 `inline def`s, 18 `transparent`), mostly `Tuple`, `NamedTuple`, `compiletime`, `Predef`, `boundary`. [V]
6. **Recommended input format: TASTy** (own reader, no dotc dependency) for both scripts and precompiled libraries, **with the reference compiler (JVM `dotc`) used out-of-process/ahead-of-time to produce TASTy for scripts** and to expand macros, while a native "mini-typer" is a later, optional investment. Details and alternatives in section 6. [I]

---

## 1. Scala 3.9 status as of September 2026 [V]

- **Scala 3.9.0 LTS was released on 3 September 2026**, "the second LTS series in the history of Scala, 3 years after its predecessor, 3.3 LTS". Guaranteed maintenance "for at least three years". 3.3 LTS gets one more year of support after 3.9.0. (scala-lang.org/news/3.9/, GitHub release 3.9.0, contributors thread.)
- **JDK 17 or newer** is required (since 3.8, kept for 3.9 and beyond; 3.3 stays JDK 8-compatible).
- **Stable language features listed for 3.9**: SIP-71 `into` (selective implicit conversions), SIP-47 clause interleaving, SIP-52 `@publicInBinary`, SIP-58 **named tuples**, SIP-62 **better-fors** (on by default), SIP-57 `runtimeChecked`, SIP-64 improved context-bound/`given` syntax. Named tuples and the new givens matter for us because their implementation leans on match types and `inline` (see §3.4).
- **Still experimental**: capture checking (`cc/` in the compiler, `CAPTURECHECKEDattr` in TASTy), SIP-61 (`@unroll`), SIP-75, SIP-68. Capture checking is described as "highly experimental and unstable" in the reference docs; issues are filed against 3.9.0 and 3.10.0-RC1. **An interpreter does not need capture checking at all**: it is a *check* phase (`cc.Setup`, `cc.CheckCaptures`) and does not change runtime semantics. [V for status; I for "not needed"]
- Infrastructure notes from the 3.9/3.8 announcements: the standard library is compiled with Scala 3 (context bounds in stdlib now desugar to `given`), Scala 2.13's TASTy reader stops consuming Scala 3.8+ artifacts, an opt-in bytecode optimizer (`-opt`), Scala CLI is the `scala` runner, JDK 26's `sun.misc.Unsafe` restriction motivated a new lazy-vals scheme (with "Sloth" support for legacy lazy-val bytecode).
- **TASTy version of 3.9.0 is 28.9-0** (`MajorVersion = 28`, `MinorVersion = 9`, `ExperimentalVersion = 0` in `tasty/src/dotty/tools/tasty/TastyFormat.scala` at tag 3.9.0; on `main` it is already 28.10-1 for 3.10). Verified on disk: `scala/collection/immutable/List.tasty` in `scala-library-3.9.0.jar` starts with `5C A1 AB 1F 9C 89 80` = header, 28, 9, 0, followed by the tooling string `"Scala 3.9.0-bin-nonbootstrapped"`.

---

## 2. The dotc phase pipeline

### 2.1 Phase list at tag 3.9.0 [V]

Source: `compiler/src/dotty/tools/dotc/Compiler.scala`. `phases = frontendPhases ::: picklerPhases ::: transformPhases ::: backendPhases`. Each inner `List(...)` is a group of mini-phases fused into a single tree traversal.

**frontendPhases** ("up to trees ready for TASTY pickling"):

| # | Phase | Role |
|---|-------|------|
| 1 | `Parser` | scanner + parser → untyped trees (`untpd`) |
| 2 | `TyperPhase` | `Namer` + `Typer` → typed trees (`tpd`). Also does *transparent* inlining and macro expansion for transparent macros (§3.3) |
| 3 | `WInferUnion`, `CheckUnused.PostTyper`, `CheckShadowing` | lint checks |
| 4 | `YCheckPositions` | debug check |
| 5 | `sbt.ExtractDependencies` | zinc callbacks |
| 6 | `semanticdb.ExtractSemanticInfo` | `.semanticdb` |
| 7 | `PostTyper` | "additional checks and cleanups after type checking" (864 lines): e.g. synthesizes case-class members (`SyntheticMembers`), normalizes `super`/`this`, checks variance, expands some annotations |
| 8 | `UnrollDefinitions` | SIP-61 (experimental) |
| 9 | `sjs.PrepJSInterop` | Scala.js only |
| 10 | `SetRootTree` | attaches root trees to class symbols |

**picklerPhases**:

| # | Phase | Role |
|---|-------|------|
| 1 | **`Pickler`** | **Generate TASTy** (serialize `tpd` trees per top-level class) |
| 2 | `sbt.ExtractAPI` | zinc API hashes |
| 3 | **`Inlining`** | "Inline and execute macros" — inlines all non-transparent `inline` calls not inside inline methods/quotes, and runs macro annotations |
| 4 | `PostInlining` | "Add mirror support for inlined code" (`deriving.Mirror` synthesis) |
| 5 | `Staging` | staging-level checks |
| 6 | `Splicing` | replace level-1 splices with holes |
| 7 | `PickleQuotes` | turn quoted trees into runtime data (`'{...}` inside macro *implementations* become pickled TASTy strings) |

**transformPhases** (lowering; 32.9 kLOC in `transform/`). Grouped by traversal, in order:

1. `InstrumentCoverage`
2. `CrossVersionChecks, FirstTransform, CheckReentrant, ElimPackagePrefixes, CookComments, CheckLoopingImplicits, BetaReduce, InlineVals, ExpandSAMs, ElimRepeated, RefChecks, DropForMap`
3. `init.Checker`
4. `ProtectedAccessors, ExtensionMethods, UncacheGivenAliases, CheckStatic, ElimByName, HoistSuperArgs, ForwardDepChecks, SpecializeApplyMethods, TryCatchPatterns, PatternMatcher`
5. `TestRecheck.Pre`, `TestRecheck` (test only)
6. `cc.Setup`, `cc.CheckCaptures` (experimental capture checking; note it runs *after* `PatternMatcher`)
7. `CheckUnused.PostPatMat`, `semanticdb.AppendDiagnostics`
8. `ElimOpaque, sjs.ExplicitJSClasses, ExplicitOuter, ExplicitSelf, StringInterpolatorOpt, DropBreaks`
9. `PruneErasedDefs, UninitializedDefs, InlinePatterns, VCInlineMethods, SeqLiterals, InterceptedMethods, Getters, SpecializeFunctions, SpecializeTuples, CollectNullableFields, ElimOuterSelect, ResolveSuper, FunctionXXLForwarders, ParamForwarding, TupleOptimizations, LetOverApply, ArrayConstructors`
10. **`Erasure`** ("Rewrite types to JVM model, erasing all type parameters, abstract types and refinements")
11. `ElimErasedValueType, PureStats, VCElideAllocations, EtaReduce, ArrayApply, sjs.AddLocalJSFakeNews, ElimPolyFunction, TailRec, CompleteJavaEnums, Mixin, LazyVals, Memoize, NonLocalReturns, CapturedVars`
12. `Constructors, Instrumentation`
13. `LambdaLift, ElimStaticThis, CountOuterAccesses`
14. `DropOuterAccessors, DropParentRefinements, CheckNoSuperThis, Flatten, TransformWildcards, MoveStatics, ExpandPrivate, RestoreScopes, SelectStatic, sjs.JUnitBootstrappers, RepeatableAnnotations`

**backendPhases**: `backend.sjs.GenSJSIR` (Scala.js only), `GenBCode` (JVM bytecode). Scala Native's `scalanative-genNIR` plugin phase declares `runsAfter = Set(transform.MoveStatics.name)`, `runsBefore = Set(backend.jvm.GenBCode.name)`, i.e. it also consumes fully lowered, post-erasure trees. [V, from `scala-native/nscplugin/.../GenNIR.scala`]

Official overview (slightly stale phase list): `docs/_docs/internals/overall-structure.md` in the repo / https://dotty.epfl.ch/docs/internals/overall-structure.html.

### 2.2 Well-defined boundaries and what the program looks like at each [V unless marked]

| Boundary | Representation | Key properties |
|---|---|---|
| **Source** | text | Scala 3 syntax incl. optional braces, `given`, `extension`, `enum`, `inline`, quotes/splices, named tuples, better-fors. |
| **After `Parser`** | `untpd` trees | Desugaring already partly done in `ast/Desugar.scala` (for-comprehensions → `map/flatMap/withFilter`, pattern-bound vals, enums → sealed hierarchies + cases, `extension` → methods, context bounds → `given` params, named tuples → `NamedTuple` types). No symbols, no types. |
| **After `TyperPhase`** | `tpd` trees | Every definition has a `Symbol` with a `Type`; every reference is resolved; overloads resolved; implicit/`given`/`using` arguments inserted as explicit `Apply` args; implicit conversions inserted as explicit calls; eta-expansion and `apply` insertion made explicit; `transparent inline` calls already expanded to `Inlined(call, bindings, expansion)`; non-transparent `inline` calls still present as ordinary `Apply` nodes to `inline def` symbols; match types and union/intersection types present in types; patterns are `Bind/Typed/UnApply/Alternative` trees with `unapply` calls resolved. |
| **After `PostTyper`** (= what `Pickler` sees) | `tpd` | Additionally: case-class synthetics, variance/override checks, some normalizations. This is the **TASTy boundary**. |
| **TASTy file** | binary tree | See §3. Semantically identical to the post-`PostTyper` trees but *term* types are not stored for most application nodes (recomputed on read). |
| **After `Inlining`/`PostInlining`/`PickleQuotes`** | `tpd` | All `inline` calls expanded; all macro splices executed and replaced by generated trees; `Mirror` instances synthesized; quotes inside macro implementations replaced by pickled TASTy strings + `Hole`s. **No compiler mode writes TASTy at this point** (`Pickler.isRunnable` is only re-run under `-from-tasty` for early-output/pipelining, where it re-emits the *already pickled* bytes; see `Pickler.runFromTasty`). |
| **After group 4 (`PatternMatcher`)** | `tpd`, still generic | Pattern matches compiled to `Labeled`/`If`/`isInstanceOf`/`asInstanceOf` decision trees; by-name params turned into `() => T` functions (`ElimByName`); SAM closures expanded to anonymous classes (`ExpandSAMs`); varargs normalized. |
| **Before `Erasure`** (after group 9) | `tpd` | opaque types dropped, outer accessors made explicit, `super` accessors resolved, getters introduced, value-class methods inlined. |
| **After `Erasure`** | `tpd` with JVM-model types | Type params, refinements, unions/intersections erased to classes; boxing/unboxing explicit; value classes to underlying; `Function0..22` and specialized variants selected. |
| **After group 11-14** | `tpd`, "flat" | tail-rec → loops; traits' fields/initializers expanded into classes (`Mixin`); `lazy val` state machines (`LazyVals`); non-local returns → exceptions; captured `var`s → `ObjectRef`/`IntRef` boxes; constructors collect field inits; nested functions lifted to methods with environment params (`LambdaLift`); inner classes flattened to package level (`Flatten`); statics moved. |
| **Backend IR** | JVM bytecode / Scala.js IR / NIR | JVM: `GenBCode` via ASM. Scala.js IR: structured, expression-based, typed, no gotos, versioned serialization (`ir/.../Trees.scala`). NIR: "very much a classical SSA-based representation", strongly typed, close to LLVM but with OO ops (class/trait/module, method dispatch, type tests). |

### 2.3 Which phases are "frontend" vs "lowering" for our purposes [I]

- *Must be reproduced by any interpreter that starts from source*: `Parser`, `Desugar`, `Namer`, `Typer` (incl. implicit search, overload resolution, inference), `PostTyper` synthetics, `Inlining` (+ macro expansion), `PostInlining` (Mirrors).
- *Must be reproduced by an interpreter that starts from TASTy*: `Inlining` (for non-transparent inline calls; §3.4), `PostInlining` (Mirrors), the semantic content of `PatternMatcher`, `ElimByName`, `ExpandSAMs`, `Mixin` (trait linearization/initialization order), `LazyVals`, `Constructors` (init order, `HoistSuperArgs`), `NonLocalReturns`, `CapturedVars`, `TailRec` (or just rely on interpreter loop), `Erasure` semantics wherever the runtime model is visible (`isInstanceOf` on generic types, `==` on boxed numerics, `Array[T]` of primitives, `getClass`, value classes).
- *Never needed*: check-only phases (`RefChecks`, `init.Checker`, `cc.*`, `CheckUnused`, `CrossVersionChecks`, `ForwardDepChecks`), sbt/semanticdb/coverage phases, JVM-specific accessor phases (`ProtectedAccessors`, `ExplicitOuter`, `Flatten`, `LambdaLift`, `ExpandPrivate`, `MoveStatics`).

---

## 3. TASTy

### 3.1 Exact structure [V]

Spec: `tasty/src/dotty/tools/tasty/TastyFormat.scala` (header comment is the normative BNF grammar, ~300 lines; the whole `tasty/src` module is 1,608 lines). The compiler's reader is `core/tasty/TreeUnpickler.scala` (1,971 lines) + `TastyUnpickler.scala` (164) + `NameBuffer`/`TastyBuffer`; the writer is `core/tasty/TreePickler.scala` (975).

```
File          = Header majorVersion_Nat minorVersion_Nat experimentalVersion_Nat VersionString UUID
                nameTable_Length Name* Section*
Header        = 0x5CA1AB1F
UUID          = Byte*16                 -- random UUID
VersionString = Utf8                    -- string that represents the compiler that produced the TASTy
Section       = NameRef Length Bytes
```

Micro-syntax: `LongInt = Digit* StopDigit` (big-endian 7-bit groups, `StopDigit = 128..255` terminates; `Nat`/`Int` are `LongInt` fitting an Int), `Utf8 = Nat UTF8-CodePoint*`.

**Name table**: `Name = UTF8 Utf8 | QUALIFIED | EXPANDED | EXPANDPREFIX | UNIQUE | DEFAULTGETTER | SUPERACCESSOR | INLINEACCESSOR | OBJECTCLASS | BODYRETAINER | SIGNED Length original_NameRef resultSig_NameRef ParamSig* | TARGETSIGNED ...`. `SIGNED` names carry the **erased signature** of a method reference (result class + param classes / type-param counts); this is how overloads are disambiguated in `SELECT`/`TERMREF` nodes. Strings are context-typed (same string can be a term or type name).

**Standard sections** (constants `ASTsSection = "ASTs"`, `PositionsSection = "Positions"`, `CommentsSection = "Comments"`, `AttributesSection = "Attributes"`):

- `"ASTs" TopLevelStat*` — the trees. Tags are grouped into 5 categories by numeric range so that any node can be skipped generically: cat 1 (1-59) `tag`; cat 2 (60-89) `tag Nat`; cat 3 (90-109) `tag AST`; cat 4 (110-127) `tag Nat AST`; cat 5 (128-255) `tag Length payload`. Node kinds: `PACKAGE, TYPEDEF, VALDEF, DEFDEF, IMPORT, EXPORT, TYPEPARAM, PARAM, EMPTYCLAUSE, SPLITCLAUSE, TEMPLATE, SELFDEF`; terms `IDENT, SELECT, SELECTin, QUALTHIS, NEW, ELIDED, THROW, NAMEDARG, APPLY, APPLYsigpoly, TYPEAPPLY, SUPER, TYPED, ASSIGN, BLOCK, INLINED, LAMBDA, IF [INLINE], MATCH (IMPLICIT | [INLINE|SUBMATCH]), TRY, RETURN, WHILE, REPEATED, SELECTouter, QUOTE, SPLICE, SPLICEPATTERN`; patterns `BIND, ALTERNATIVE, UNAPPLY, QUOTEPATTERN`; type trees `IDENTtpt, SELECTtpt, SINGLETONtpt, REFINEDtpt, APPLIEDtpt, LAMBDAtpt, TYPEBOUNDStpt, ANNOTATEDtpt, MATCHtpt, BYNAMEtpt, SHAREDterm`; and `EXPLICITtpt, HOLE` which "can only appear in pickled quotes. They will never be in a TASTy file". Paths/constants: `TERMREFdirect, TERMREFsymbol, TERMREFpkg, TERMREF, TERMREFin, THIS, RECthis, SHAREDtype`, `UNITconst..CLASSconst`. Types: `TYPEREF*`, `RECtype, SUPERtype, REFINEDtype, APPLIEDtype, TYPEBOUNDS, ANNOTATEDtype, ANDtype, ORtype, MATCHtype, MATCHCASEtype, FLEXIBLEtype, BIND, BYNAMEtype, PARAMtype, POLYtype, METHODtype, TYPELAMBDAtype`. Modifiers include `PRIVATE..OVERRIDE, OPAQUE, INLINE, MACRO ("Inline method containing toplevel splices"), INLINEPROXY, STATIC, OBJECT, TRAIT, ENUM, LOCAL, SYNTHETIC, ARTIFACT, MUTABLE, FIELDaccessor, CASEaccessor, COVARIANT, CONTRAVARIANT, HASDEFAULT, STABLE, EXTENSION, PARAMsetter, PARAMalias, EXPORTED, OPEN, INVISIBLE, TRACKED, INTO`, plus `ANNOTATION`.
- `"Positions" LinesSizes Assoc*` — delta-encoded (addr, start, end, point) per node, plus `SOURCE nameref` markers; "Nodes which have the same positions as their parents are omitted."
- `"Comments" Comment*` — `Utf8 LongInt` doc comment + coordinates.
- `"Attributes" Attribute*` — boolean flags `SCALA2STANDARDLIBRARYattr=1, EXPLICITNULLSattr=2, CAPTURECHECKEDattr=3, WITHPUREFUNSattr=4, JAVAattr=5, OUTLINEattr=6` and `SOURCEFILEattr=129 Utf8Ref`. Ordered by tag, no repeats. (`OUTLINEattr` marks pipelining "outline" TASTy that only has signatures; `-from-tasty` refuses it unless `-Xallow-outline-from-tasty`.)

Important grammar details for a reader:

- The type of a `SELECT` is *not* stored: the reader does `completeSelect(name, sig, target)` — a member lookup on the qualifier's type using the signed name (`TreeUnpickler.scala` ~L1357). `APPLY`, `TYPEAPPLY`, `BLOCK`, `IF`, ... also carry no type; `TreeUnpickler` re-derives them via `TypeAssigner` (`ctx.typeAssigner.maybeSkolemizePrefix`, `asSeenFrom`, `appliedTo`). Only `IDENT` (`NameRef Type`), definitions, `TYPED`, `BIND`, `UNAPPLY`, `REPEATED`, `SELECTouter`, `QUOTE/SPLICE`, `ELIDED`, `INLINED`'s bindings etc. store types explicitly. This is the "enough information is stored to be able to *reconstruct* everything (in particular, the types of subexpressions in method bodies)" caveat in tasty-query's README. [V]
- Term references use `TERMREFdirect sym_ASTRef` for locals (an address into the AST payload) and `TERMREF possiblySigned_NameRef qual_Type` for non-local members: cross-file references are *by name + signature*, not by symbol id, so a reader needs a classpath-wide symbol table and lazy loading. [V]
- Inline method bodies are stored as ordinary `DEFDEF` rhs (that is why full bodies are in TASTy; `BODYRETAINER` names/`@bodyAnnotation` exist for the binary-compat "retained body" methods). [V]

### 3.2 Version numbering and stability guarantees [V]

From `TastyFormat.scala` (tag 3.9.0):

- `MajorVersion = 28`: "Each increment of the `MajorVersion` begins a new series of backward compatible TASTy versions. A TASTy file in either the preceding or succeeding series is incompatible with the current value."
- `MinorVersion = 9`: "Each increment of the `MinorVersion`, within a series declared by the `MajorVersion`, breaks forward compatibility, but remains backwards compatible, with all preceding `MinorVersion`."
- `ExperimentalVersion = 0`: 0 = stable release; > 0 = experimental, readable only by the same experimental version (but experimental tooling can read stable files with strictly lower minor).
- Compatibility relation: `fileMajor == compilerMajor && (fileMinor == compilerMinor && fileExperimental == compilerExperimental || fileMinor < compilerMinor && fileExperimental == 0)`.
- Mapping to language versions (binary-compatibility reference doc): "a bump in language minor version corresponds to a bump in TASTy minor version (e.g. for Scala 3.0.0 the TASTy version is 28.0-0)". So 3.3 LTS = 28.3, 3.8 = 28.8, **3.9 LTS = 28.9**, 3.10 (in development, `main`) = 28.10-1.
- Consequence for us: a reader written for 28.9 can read every stable TASTy from 3.0 to 3.9 (`fileMinor <= 9`) but a 3.10 library will be rejected by an unmodified reader; the format is only *extended* between minors (new tags/attributes), never re-laid-out within major 28. The 3.9 announcement also states "Libraries built with 3.9 work with later 3.9 patches and Scala Next releases" and that 3.3 artifacts are consumable by 3.9 but not vice versa. [V]
- tasty-query on `main` also hard-codes `MajorVersion = 28, MinorVersion = 9` and the same `isVersionCompatible`; its latest tags are v1.7.0..v1.9.0. [V]

### 3.3 What TASTy carries and what it does not [V]

| Question | Answer | Evidence |
|---|---|---|
| Full types? | Yes for all definitions, parameters, type trees and local references; **recomputed** for `Select/Apply/...` nodes. Types are *pre-erasure*: generics, unions, intersections, match types, refinements, singleton types, `TYPELAMBDAtype`, `FLEXIBLEtype` (explicit nulls) are all present. | `TastyFormat.scala` grammar; `TreeUnpickler.completeSelect` |
| Resolved implicits/givens? | Yes: after typer they are ordinary explicit arguments (`APPLY` with `using`-clause args; `IMPLICITARG` for unapply); implicit conversions are explicit calls. `MATCH IMPLICIT` marks `summonFrom`-style implicit matches *inside unexpanded inline bodies* — those are only resolved at inlining time. | pipeline position of `Pickler` after `TyperPhase`; grammar |
| Overload resolution? | Yes: `SIGNED`/`TARGETSIGNED` names on `SELECT`/`TERMREF` encode the chosen erased signature. | grammar |
| Inlined code? | **Partially.** `Inlines.needsInlining` (inlines/Inlines.scala L66-88) inlines at `typerPhase` only if `needsTransparentInlining` (symbol is `Transparent` or `Mode.ForceInline`), otherwise at `inliningPhase`. `Pickler` runs between them. So: `transparent inline` calls → `INLINED` nodes in TASTy; plain `inline` calls → **unexpanded `APPLY` to an `INLINE` symbol**; the `INLINED` node's `call_Term?` keeps the original call for positions. | `Compiler.scala`, `Inlines.scala`, `Typer.scala` L4859, `transform/Inlining.scala` |
| Macro expansion? | Same rule: a macro is `inline def f = ${ impl }`. If `transparent`, expanded in typer → expansion in TASTy. Otherwise the splice is executed in the `Inlining` phase → **TASTy contains the unexpanded macro call**. | same |
| Produced before or after inlining? | **Before** (`Pickler` precedes `Inlining` in `picklerPhases`). There is no supported flag to pickle post-inlining trees. `-from-tasty` compilation (`fromtasty/TASTYCompiler.scala`: `frontendPhases = List(new ReadTasty)`) re-runs `Inlining` and everything after on the unpickled trees. | `Compiler.scala`, `Pickler.scala`, `fromtasty/` |
| Pattern matching lowered? | No: `MATCH`/`CASEDEF`/`UNAPPLY`/`BIND`/`ALTERNATIVE` are source-level. | grammar |
| Erasure applied? | No. | grammar, phase order |
| Lazy vals, by-name, context functions, traits? | Source-level: `LAZY` modifier, `BYNAMEtype`/`BYNAMEtpt`, context-function types as `AppliedType(ContextFunctionN, ...)` with `LAMBDA` closures, traits as `TEMPLATE`s with parents. | grammar |
| Positions, docs, source file | Yes (sections 2-4), so error messages and stack traces with lines are possible. | grammar |
| Scala 2 stdlib / Java classes | Not in TASTy; Scala 2 pickles and Java classfiles must be read separately (tasty-query has `PickleReader` 1,239 lines and `ClassfileReader/Parser` ~1,200 lines for that). **Since 3.8 the stdlib is Scala 3-compiled, so this is only needed for third-party Scala 2.13 libraries and for JDK classes.** | tasty-query source; `scala-library-3.9.0.jar` contents |

### 3.4 What a TASTy-based interpreter has to do beyond reading trees [V facts, I on effort]

1. **Classpath symbol table + lazy loading**: resolve `TERMREF`/`TYPEREF` by fully-qualified name and erased signature across jars; resolve `TERMREFdirect` by AST address; handle `SHAREDterm/SHAREDtype` back-references. (tasty-query: `Symbols.scala` 1,985 lines, `reader/` ~5,000 lines incl. Scala 2 pickles and classfiles.)
2. **Type reconstruction "light"**: member lookup by name+signature on a (possibly generic, intersection, refined) qualifier type; `asSeenFrom`/substitution for type params; dealiasing; *some* subtyping (tasty-query: `Types.scala` 2,457 lines, `Subtyping.scala` 572). An interpreter needs less than tasty-query (it never has to *check* anything) but must: pick the right overload target (signature is given, so this is a lookup, not resolution); decide which `Function`/`ContextFunction` arity/specialization a `LAMBDA` targets; know class linearization for `super` and mixin init order.
3. **Inlining** for every non-transparent `inline def` reachable from the script (see §3.3). The reference implementation is `inlines/Inliner.scala` (1,317 lines) + `InlineReducer.scala` (456) + `Inlines.scala` (668) + `PrepareInlineable.scala` (339). Hard parts: `inline if`/`inline match` require *constant folding and type-level reduction* (match types); `summonInline`/`summonFrom` require **implicit search at the expansion site**; `constValue`/`erasedValue`/`compiletime.ops` require match-type normalisation; the inlined body must be re-typed (the inliner calls the typer on the substituted body). **Mitigation**: the 3.9 stdlib has only 63 `inline def`s (18 transparent) in 9 files: `compiletime/package.scala`, `compiletime/testing/package.scala`, `NamedTuple.scala`, `Predef.scala` (e.g. `assert`, `summon`, `nn`), `quoted/Quotes.scala`, `runtime/stdLibPatches/Predef.scala`, `runtime/Tuples.scala`, `Tuple.scala`, `util/boundary.scala`; 27 uses of `summonInline|summonFrom|constValue|erasedValue`, 21 match-type definitions (all counts from `scala-library-3.9.0-sources.jar`). So "scripts using collections/strings/IO" mostly hit `Predef.assert`/`summon`/`boundary`, which are trivially inlinable, while `Tuple`/`NamedTuple` generic ops are the hard tail. Third-party libraries (e.g. circe/upickle/zio derivations, `Mirror`-based type-class derivation) are heavy users of inline + macros. [I]
4. **`Mirror` synthesis** (`PostInlining`, `Synthesizer.scala` 861 lines in typer): `summonInline[Mirror.Of[T]]` is synthesized by the compiler for case classes/enums; anything using type-class derivation needs it.
5. **Pattern matching semantics** (`PatternMatcher.scala` 1,156 lines as reference, but a direct interpreter of `MATCH`/`UNAPPLY` trees is much simpler): type tests against *erased* types (`case x: List[Int]` tests `List`), `unapply`/`unapplySeq` protocol (`isEmpty/get`, name-based extractors, `Product` extraction, `Seq` extraction), guards, alternatives, `@unchecked`, `MatchError`.
6. **Erasure-visible runtime semantics** (`TypeErasure.scala` 1,114 lines): `isInstanceOf`/`asInstanceOf` on generic/union/intersection types, boxing (`Int` vs `java.lang.Integer`), `==` (`BoxesRunTime.equals` cooperative equality for numerics), value classes (`AnyVal` subclasses erase to underlying), `Array[T]` with `ClassTag`, `getClass`, `Function` arity classes, `Nothing`/`Null`.
7. **Object model**: class linearization (`Mixin`, `ResolveSuper`), trait initialization order, `HoistSuperArgs`, primary/secondary constructors, `lazy val` (thread-safety optional in scripts), `object` initialization (lazy, once), `ExplicitOuter` semantics for inner classes/closures, `CapturedVars`, non-local `return` (exception-based), `try/finally`, `boundary/break` (`DropBreaks` is an optimisation only), `while`, `@tailrec` (interpreter loop can handle deep recursion with its own stack), `by-name` params (`ElimByName` → thunks), context functions (closure with implicit params), default arguments (`DEFAULTGETTER` methods), varargs (`REPEATED` → `Seq`), string interpolators (`StringContext.s/f/raw`; `StringInterpolatorOpt` is an optimisation), `enum` values/ordinals (synthesized in `Desugar` + `SyntheticMembers`, so already in TASTy).
8. **Java interop boundary**: everything that is not TASTy (JDK, Java libraries, Scala 2.13 artifacts) needs either a JVM (out of scope), a re-implementation of the used JDK surface (Scala Native's `javalib` approach), or a Scala-2-pickle + classfile reader plus an FFI. This is the same problem Scala Native solved by re-implementing `java.*` in Scala (`javalib`) and is presumably the reason for choosing the Scala Native subset as compatibility target. [I]

### 3.5 Practical stdlib availability [V]

- `org.scala-lang:scala-library:3.9.0` (9.8 MB): 3,662 `.class`, **941 `.tasty`** (139 in `scala/`, 105 in `scala/collection/immutable`, 89 `scala/collection`, 87 `scala/runtime/java8`, 80 `scala/runtime`, 62 `scala/collection/mutable`, ...). TASTy header: 28.9-0, tooling string `Scala 3.9.0-bin-nonbootstrapped`.
- `org.scala-lang:scala3-library_3:3.9.0` is a 344-byte jar with 0 classes whose POM depends on `scala-library:3.9.0`. (Before 3.8, `scala3-library_3` held the Scala-3-only parts like `scala.quoted`, `Tuple`, and `scala-library` was the 2.13 jar without TASTy.)
- Some stdlib sources are still Scala 2-syntax (`SCALA2STANDARDLIBRARYattr` exists for this: the stdlib is compiled with `-Yscala2-stdlib`-style compat), and some runtime pieces (`scala/runtime/java8/*` function specializations, `BoxesRunTime`, `ScalaRunTime`, `Statics`) are Java or JVM-specific. Scala Native maintains its own patched `scalalib`/`javalib` for exactly this reason. [V for attr and file list; I for the rest]

---

## 4. Macros in Scala 3

### 4.1 The language model [V]

Reference: https://docs.scala-lang.org/scala3/reference/metaprogramming/macros.html, `library/src/scala/quoted/Quotes.scala` (6,083 lines; the `scala.quoted` package is 8,009 lines total).

- A macro is `inline def m(x: T): R = ${ impl('x) }` where `impl(x: Expr[T])(using Quotes): Expr[R]` is an ordinary method. "The top-level splice must contain a single call to a compiled static method"; allowed splice arguments are literals, quoted expressions (`'x`), `Type.of[T]`, and the `Quotes` instance — not nested splices.
- Quotes `'{...}` : `Expr[T]`, `Type[T]`; staging levels = #quotes − #splices; locals accessible only at their own level (phase-consistency); `Quotes.reflect` exposes the compiler's `Tree/Term/TypeRepr/Symbol` API (the "reflection API"), implemented in the compiler by `compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala` (3,651 lines) on top of dotc's real data structures.
- **Macro implementations must be compiled in a previous run** (separate module or an earlier compilation of the same project — the compiler suspends units whose macro deps are being compiled in the same run: `CompilationUnit.suspend`, `-Yno-suspended-units`). Also: `inline` alone (no splice) is *not* a macro: it is pure tree substitution done by the inliner, plus `scala.compiletime` intrinsics.
- Macro annotations (`@experimental class foo extends MacroAnnotation`) exist (`transform/MacroAnnotations.scala`), also run through `Interpreter`.
- Scala 2 macros are *not* supported (`Typer` emits an error or, with `-Xignore-scala2-macros`, a runtime `MatchError` stub).

### 4.2 How the compiler runs macros today [V]

Call chain at tag 3.9.0:

1. `Inlines.inlineCall` (from `Typer.adapt` for transparent, from `transform.Inlining` for the rest) → `Inliner` substitutes the inline body; when the body is a top-level splice it calls `Splicer.splice(tree, splicePos, spliceExpansionPos, MacroClassLoader.fromContext)`.
2. `transform/Splicer.splice` (Splicer.scala L45-87): creates a `MACRO` owner symbol, sets the thread context classloader, builds `new SpliceInterpreter(splicePos, classLoader)`, calls `interpreter.interpret[Quotes => scala.quoted.Expr[Any]](tree)`, then `PickledQuotes.quotedExprToTree(macroClosure(QuotesImpl()))` to turn the returned `Expr` back into a `tpd.Tree`, then `checkEscapedVariables`. Errors: `StopMacroExpansion` (macro reported errors), `StopInterpretation` (unsupported tree or runtime exception in macro), `SuspendException` (macro class not yet compiled → suspend the unit and retry after the dependency compiles), other exceptions → "Failed to evaluate macro".
3. `quoted/Interpreter.scala` (392 lines) — `class Interpreter(pos, classLoader0)`: "Tree interpreter for metaprogramming constructs". `interpretTree` handles exactly: `Literal`; `Ident` of an `inline val` (constant type); `Call(fn, args)` where `fn` is a constructor (`interpretNew` → `Class.getConstructor(...).newInstance`), a module (`loadModule` → `MODULE$` field or `getConstructor().newInstance()` for nested objects), a static method (`interpretedStaticMethodCall` → `clazz.getMethod(name, paramClasses).invoke(inst, args)` with parameter classes computed from `TypeErasure.erasure(sym.info)` plus context-function extra params), a static field, an env-bound local, or an `InlineProxy`; single-arg `closureDef` → a JVM `Function1` lambda; `Block`/`NamedArg`/`Inlined(_, bindings, expansion)` → sequential `ValDef` environment; `Typed`; `SeqLiteral` → `Seq`. **Everything else throws `StopInterpretation("Unexpected tree could not be interpreted")`**. By-name args become thunks (`ExprType`). `SpliceInterpreter` adds two cases: level −1 quotes `'{body}` → `new ExprImpl(Inlined(...body...))` and `Type.of[T]` → `new TypeImpl(...)`.
4. `core/MacroClassLoader.scala`: a `URLClassLoader` over `ctx.settings.classpath` (recomputed per call in interactive/REPL mode). `MissingClassValidInCurrentRun` maps `NoClassDefFoundError` to either "suspend and compile the dependency first" (source origin) or a hard error mentioning pipelining (classpath origin, e.g. outline TASTy with no `.class`).

So: **the "TASTy interpreter" inside dotc interprets only the glue** (argument evaluation and the static call shape) — the macro *body* always runs as **JVM bytecode** via reflection, and the `Quotes` it receives is `QuotesImpl`, i.e. live access to the compiler's `Context`, `Types`, `TypeComparer`, `Symbols` and the unpickler for quoted code. This is why `dotty.tools.dotc.quoted.Interpreter` does *not* help a non-JVM host: the hard part (running arbitrary Scala, and giving it a working reflection API) is delegated to the JVM and to the whole compiler. [V + I]

### 4.3 What it would take to expand macros in a non-JVM host [I, grounded in the above]

There are essentially four options:

| Option | What it needs | Assessment |
|---|---|---|
| **A. Delegate to a JVM `dotc` at "compile" time** (out-of-process, cached, ahead-of-time) | Nothing new in the interpreter; scripts are compiled to TASTy + fully inlined by the reference compiler once; interpreter reads TASTy. Requires a JVM on the dev machine (or a GraalVM native-image of `dotc`, which the GraalVM blog shows is feasible but "macros are dynamically loaded, so a native image cannot support all macros in the world"). | Only route to *full* macro compatibility today. Startup cost is paid once per script change if the artifact is cached (`~/.cache/...` keyed by source hash + classpath hash). But note the catch in §3.3: **dotc never writes post-inlining TASTy**, so the interpreter still has to inline itself, or we add a small compiler plugin/patched `Pickler` that pickles after `PickleQuotes` (technically simple: the phase order is a `List`; `-Ytest-pickler` already re-pickles for checks; `Pickler.scala` and `TreePickler` handle post-inlining trees as long as no `HOLE`/quotes remain — `PickleQuotes` is *designed* to turn quotes into runtime data, so after it the trees are ordinary). |
| **B. Interpret macro implementations in our own interpreter** | Our interpreter must be able to run the macro impl's TASTy (it is a normal Scala method — fine) **and provide `scala.quoted.Quotes`**: an implementation of the `reflect` API (`QuotesImpl` is 3,651 lines on top of the whole compiler: `TypeRepr.<:<`, `=:=`, `widen`, `dealias`, `Symbol.memberMethods`, `Implicits.search`, `TypeRepr.of`, term construction with type assignment, `Expr.betaReduce`, `report.error`, pattern matching on quotes (`QuotePatterns.scala`, 338 lines)), plus quote (un)pickling (`PickledQuotes.scala` 291 lines: quoted trees are stored as TASTy strings + holes and spliced at expansion). | Feasible for a *subset* (many macros only build/inspect terms, read `Expr[String]` constants, summon givens, inspect case-class fields via `Symbol.caseFields`/`Mirror`). But `Implicits.search` and `<:<` are the typer. This option converges on "reimplement the typer". |
| **C. Precompiled macro implementations as native plugins** (macro authors ship WASM/dylib) | A new ecosystem convention; nobody does this today. | Not realistic for 3rd-party libs; maybe for a curated set (e.g. our own scripting DSLs). |
| **D. Restrict the supported subset** (no macros; only `inline` without splices; explicit derivation instead of `Mirror`) | Inliner subset as in §3.4 item 3. | Scala Native's test suite itself uses macros in a few places (e.g. `scala.scalanative.unsafe` extern/`CFuncPtr` macros are compiler intrinsics in nscplugin, and `sourcecode`/`munit` in tests). [I: check in later research] |

Recommended path: **A now** (with the pickle-after-inlining plugin as a small deliverable), **D as the "no JVM present" fallback**, **B incrementally for the most common macro idioms** if user demand appears. [I]

### 4.4 Prior work [V]

- **`tests/old-tasty-interpreter-prototype/`** in scala/scala3 (still present at tag 3.9.0): `interpreter/TastyInterpreter.scala` (28 lines), `TreeInterpreter.scala` (219), `jvm/Interpreter.scala` (174), `jvm/JVMReflection.scala` (116), `notes.md`. A 2019-era prototype (originally `tests/run-with-compiler-custom-args/tasty-interpreter`) written against the `scala.quoted.Quotes.reflect` API: abstract `TreeInterpreter[Q <: Quotes]` with `Env = Map[Symbol, LocalValue]` (val/var/lazy), `interpretCall` evaluating `DefDef.rhs` for symbols "defined in current run", `interpretIf/While/Block`; the JVM subclass falls back to `java.lang.reflect` for precompiled classes and builds `java.lang.reflect.Proxy` objects for interpreted classes extending precompiled traits. `notes.md` lists the open design points: "Abstract platform operations (Arrays); Proxies (environment of the object, `this` in Env, class with fields, custom/secondary constructors); Stack (local def env/closures, local class env)". It never went further; it is useful as a checklist of what was found hard, not as code to reuse.
- **`dotty.tools.dotc.quoted.Interpreter`** — the production splice interpreter described in §4.2 (JVM reflection based).
- **`-from-tasty` / `dotty.tools.dotc.fromtasty`** (283 lines): proves TASTy is a complete input for `Inlining` → backend; `scala3-tasty-inspector` (`scala.tasty.inspector.TastyInspector`) exposes the same via the `Quotes.reflect` API for tooling; `-decompile`/`-print-tasty` (`decompiler/`) pretty-prints TASTy.
- **scalacenter/tasty-query** (18,806 lines of shared Scala, cross-built for **JVM and Scala.js** only — `build.sbt` has `crossProject(JSPlatform, JVMPlatform)`, no Native): "compiler-independent library to semantically analyze TASTy": symbols, types (with `Subtyping.scala`, type equivalence, member lookup, overriding relationships), full trees for Scala 3 code, readers for TASTy 28.9, Scala 2 pickles and Java classfiles. Explicitly *no* typer and *no* implicit resolution. Latest tags v1.7.0–v1.9.0. It is the best existing blueprint for the "TASTy reader + type model without a typer" component, and its size (~19 kLOC Scala, of which ~5 kLOC are Scala 2/Java readers) is a realistic lower bound for ours. It is written in Scala, so for a Rust host it is a design reference, not a dependency (unless we run it via Scala.js/Native — Scala Native is not a build target). Built for **tasty-mima** (binary/TASTy-compat checker) and used by the Scala Debug Adapter's expression evaluator.
- **Best-effort compilation** (`docs/_docs/internals/best-effort-compilation.md`, Scala 3.5+): `-Ybest-effort` writes `.betasty` files (different header, adds `ERRORtype`) to `META-INF/best-effort` even for erroring code, running `Parser → TyperPhase → (ExtractSemanticDB) → Pickler` and then stopping; `-Ywith-best-effort-tasty` lets the presentation compiler read them, restricted to frontend phases. Relevant only as evidence that "typer output → TASTy" is a clean cut in the compiler; not useful for execution.
- **Pipelining** (`-Xearly-tasty-output`/`-Ypickle-write`, `-Xjava-tasty`/`-Ypickle-java`, sbt `usePipelining`, PR scala/scala3#18880): early TASTy emitted right after `Pickler` on a background thread; downstream projects can *type-check* against it but macro execution still needs `.class` files (`Interpreter.suspendOnMissing` error text: "possibly it indicates that it comes from a pipeline-compiled dependency"). This is a live demonstration that TASTy-without-classfiles is enough for the frontend but not for running macros. Several open issues in Sept 2026 (scala/scala3#27125, #27133, #27139; sbt/sbt#9792).
- **Scala.js / Scala Native**: both run the JVM compiler unchanged for everything before their code-gen phase (`GenSJSIR` is in `backendPhases`; `scalanative-genNIR` runs after `MoveStatics`), so macros always expand on the JVM at compile time and the emitted IR is post-macro, post-erasure. Neither has any non-JVM macro story. Scala Native's own `unsafe` intrinsics are handled inside `nscplugin`, not as user macros.
- A personal write-up "Building a TASTy interpreter" (etaoin-shrdlu.xyz) exists but was not reachable (bot-protected) at the time of writing; not relied upon.

---

## 5. How much of the typer is realistically reimplementable?

### 5.1 Size at tag 3.9.0 [V] (`wc -l` on `compiler/src/dotty/tools/dotc/**/*.scala`; total dotc = 179,725 lines)

| Package | Lines | Files | Notes |
|---|---:|---:|---|
| `typer/` | 24,835 | 27 | `Typer.scala` 5,421; `Applications.scala` 3,062 (overloading, application, unapply typing); `Namer.scala` 2,332; `Implicits.scala` 2,227; `RefChecks.scala` 2,172 (check-only); `Checking.scala` 1,840 (check-only); `ProtoTypes.scala` 1,100; `Inferencing.scala` 904; `Synthesizer.scala` 861 (Mirrors, `ClassTag`, `=:=`, `CanEqual`, `TypeTest`...); `Nullables.scala` 636; `TypeAssigner.scala` 600; `QuotesAndSplices.scala` 394; `Deriving.scala` 328; `Dynamic.scala` 285; `EtaExpansion.scala` 281 |
| `core/` | 46,447 | 86 | `Types.scala` 7,318; `TypeComparer.scala` 4,113 (subtyping incl. unions/intersections/match types/GADT bounds); `SymDenotations.scala` 3,157; `Definitions.scala` 2,590; `Denotations.scala` 1,393; `Contexts.scala` 1,144; `TypeErasure.scala` 1,114; `TypeOps.scala` 1,078; `Symbols.scala` 1,070; `OrderingConstraint.scala` 1,022 + `ConstraintHandling.scala` 916 (inference constraints); `core/tasty/` (unpickler 1,971, pickler 975) |
| `inlines/` | 2,780 | 4 | inliner + reducer |
| `cc/` | 10,184 | 15 | capture checking (experimental; skip) |
| `parsing/` | 10,854 | 14 | scanner/parser/`JavaParsers` |
| `ast/` | 9,696 | 12 | trees, `Desugar.scala`, `TreeInfo` |
| `transform/` | 32,873 | 124 | lowering (see §2) |
| `quoted/` + `staging/` | 1,192 + 505 | | splice interpreter, quote pickling, staging checks |
| `reporting/` | 6,441 | 20 | messages |
| `semanticdb/` | 12,717 | 40 | irrelevant |

`scala.quoted` runtime: `library/src/scala/quoted/Quotes.scala` 6,083 lines (API), `compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala` 3,651 (implementation).

For comparison, tasty-query implements symbols + types + subtyping + readers with no typer in 18.8 kLOC.

### 5.2 The hard parts (why "just port the typer" is a trap) [I, informed by file sizes and the reference docs]

- **Implicit/given search** (`Implicits.scala` 2.2k + parts of `Typer`/`ProtoTypes`): contextual scope rules, implicit scope of a type (companions of all parts of the type, incl. of type args, prefixes, supertypes), specificity/ranking (SIP-64 changed the rules in 3.7+), divergence checking, by-name implicits, `NotGiven`, `summonFrom`, ambiguity errors, caching. Must match the reference compiler *exactly* or programs silently change meaning (e.g. which `Ordering` or `Conversion` is picked).
- **Type inference with union/intersection types** (`Inferencing`, `OrderingConstraint`, `ConstraintHandling`, `TypeComparer`): local type inference with constraint solving, `WInferUnion` widening rules, hard/soft unions, `Nothing`-avoidance, `FLEXIBLEtype` under explicit nulls.
- **Match types** (`TypeComparer`/`MatchTypeTrace`, spec SIP "match types" with disjointness checks): needed both for typing and, unavoidably, for **inlining** `Tuple`/`NamedTuple` ops and `compiletime.ops`.
- **GADTs** (`GadtConstraint`, `docs/_docs/internals/gadts.md`): pattern-bound type constraints during typing of case bodies. Not needed for *execution* (TASTy already has the resolved trees) but needed for a source typer.
- **Overloading and applications** (`Applications.scala` 3k): default args, named args, varargs, auto-tupling, `apply`/`update` insertion, extension method resolution (extension methods are resolved *by the typer* as a special lookup with fallback to implicit scope — `extension`s are plain methods with `EXTENSION` flag in TASTy, so a TASTy interpreter gets them for free).
- **Dependent method types, path-dependent types, refinements, singleton types, opaque types** all interact with subtyping.
- **Capture checking**: experimental in 3.9; skip entirely (does not affect runtime semantics).
- The typer is also the thing that **evolves fastest** between minors (3.7 → 3.9 changed given priority, into, named tuples, better-fors); tracking it in a second implementation is a permanent tax. TASTy, by contrast, only *gains* tags between minors.

**Bottom line** [I]: a from-scratch Scala 3 typer that agrees with dotc on real-world code is on the order of 40-60 kLOC of Rust plus years of conformance work (the dotc typer+core is ~70 kLOC of dense Scala and has 10+ years of bug-compat behind it). A **"reader-typer"** — the subset needed to *consume* TASTy (member lookup with signatures, `asSeenFrom`, dealias, erasure, class linearization, simple subtyping for `isInstanceOf`) — is ~10-15 kLOC (tasty-query's `Types`+`Subtyping`+`Symbols` are ~5 kLOC of Scala). Adding an **inliner without implicit search** (`inline if/match` on constants, `constValue`, simple substitution) is perhaps another 3-5 kLOC; adding `summonInline` and match-type reduction pulls in implicit search and `TypeComparer`-grade subtyping and is where the cliff is.

---

## 6. Candidate interpreter input formats

Goals recap: fast startup for scripts (no JVM at run time), Scala-Native-subset compatibility validated by Scala Native's test suites, later reuse of precompiled libraries (stdlib first), macro support desirable, JVM bytecode compat optional.

| Input | Pros | Cons | Macro support | Precompiled libs | Startup |
|---|---|---|---|---|---|
| **Raw source + own parser/typer** | True zero-dependency scripting (no JVM ever). Best possible startup (parse+type a small script in ms). Own error messages. | Must reimplement Parser (11 kLOC ref), Desugar, Namer/Typer, implicit search, inference, match types, Synthesizer, PostTyper synthetics, inliner. Semantics drift vs dotc; huge conformance surface; every 3.x minor changes the typer. Libraries still come as TASTy, so you need the TASTy reader *anyway*. | Only via option B (own `Quotes`), i.e. more typer. | Via TASTy reader (needed anyway). | Excellent |
| **TASTy** (28.9) | Stable, documented, versioned format with a 1.6 kLOC spec; full pre-erasure typed trees; all implicits/overloads resolved; positions/docs for diagnostics; **whole stdlib ships as TASTy since 3.8**; all Scala 3 libraries on Maven ship TASTy; same input the reference compiler uses for `-from-tasty`; tasty-query is a blueprint. Interpreter never has to *check* anything. | Not erased: need erasure-visible semantics; need pattern-match, mixin, lazy-val, by-name semantics; **need inliner** for non-transparent `inline` (incl. macros unless pre-expanded); type reconstruction for `Select` (member lookup with signature). Non-Scala-3 deps (JDK, Scala 2.13 libs) need a classfile/pickle reader + FFI or a Scala-implemented `javalib` à la Scala Native. TASTy minor bumps every ~6 months need reader updates (additive). Scripts must first be turned into TASTy by *some* typer (dotc on a JVM, cached). | Pre-expanded if we pickle after `PickleQuotes` with a tiny dotc plugin (§4.3 A); otherwise only inline-without-splices. | Yes, first-class. | Script: parse-free — loading a few hundred KB of TASTy for a script + lazily the stdlib classes it touches; likely tens of ms [I]. Needs a warm cache of compiled TASTy for the script itself. |
| **Post-erasure dotc trees** (e.g. dumped after `Erasure`/`LambdaLift` via a compiler plugin, custom serialization) | Lowering already done by dotc (pattern matching, mixins, lazy vals, by-name, erasure, closures); interpreter becomes a simple class/method/expression evaluator; exactly the Scala Native/Scala.js entry level. | **No standard serialization**; would have to invent a format and a plugin, and rebuild every library with it (no ecosystem artifacts). Loses generic types (only matters for reflection-like features). Trees at that level are JVM-shaped (accessors, outer pointers, `$init$`, `Function` specializations, boxing), not "Scala". | Pre-expanded. | Only if re-built with our plugin. | Good once produced; producing needs a JVM. |
| **Scala Native NIR** | Stable-ish serialized IR that already exists for every Scala Native artifact (`.nir` in jars); post-erasure, post-lowering, SSA, strongly typed, with OO ops (classes, traits, modules, virtual dispatch, type tests) — trivially interpretable or JIT-able; `javalib`/`scalalib`/`nativelib` exist as NIR, solving the "JDK classes" problem; **exactly the Scala Native test-suite subset by construction**. | NIR is an implementation detail of Scala Native (no spec beyond `contrib/nir.html` docs and the Scala `nir` module; version churn with each 0.x); built from JVM dotc + nscplugin, so requires the JVM toolchain to produce; scripts need the full SN pipeline (dotc + plugin) but *not* the LLVM link step. Low-level (SSA, explicit unwinding/`unwind` labels, GC-aware allocation, `ptr` ops) — interpreter must model memory-ish things (`stackalloc`, `Ptr`, `CFuncPtr`, `extern`). Semantics tied to SN runtime (`Intrinsics`, `Boxes`). | Pre-expanded. | Yes (every `_native0.5_3` artifact). | Good; NIR is compact binary. |
| **Scala.js IR (SJSIR)** | Well-specified, versioned, serialized, structured (no goto), expression-based, typed IR with a clean semantics document; every `_sjs1_3` artifact ships it; `scala.js` `javalib` is available as SJSIR; a mature reference interpreter exists (`scala-js-ir-interpreter` / Scala.js "IR interpreter" and the linker's optimizer) and a WASM backend. | JS-flavoured semantics (JS numbers, `JSNew`, `JSSelect`, `undefined`), `Long` emulation, JS interop nodes; not the Scala Native subset (SN target implies OS/IO/FFI). Produced by JVM dotc + `scalajs-compiler` plugin. | Pre-expanded. | Yes. | Good. |
| **JVM bytecode** | Every Scala/Java library ever; simplest widely known semantics; existing readers in every language; a bytecode interpreter/JIT is well-trodden (this is "write a JVM"). | It *is* writing a JVM: class loading/linking/verification, exceptions, `invokedynamic`/`LambdaMetafactory` (Scala 3 lambdas), string concat indy, `MethodHandle`s, the whole `java.base` runtime (or reuse OpenJDK's classes, which drag in native methods). Erased, no Scala-level info (fine for execution). No Scala-Native-subset alignment; JDK 17+ bytecode features. Startup dominated by class loading, same problem the JVM has (unless AOT/CDS-like caching). | Pre-expanded. | Yes, maximal. | Mediocre without a lot of engineering (that is why the JVM is slow to start). |

### 6.1 Recommendation [I]

1. **Primary input: TASTy 28.9**, read by our own Rust reader (design after tasty-query: name table → lazy symbol table over the classpath → typed trees; ~15-20 kLOC Rust estimate for reader + type model + erasure + linearization). This is the only format that is (a) specified and stable within a major, (b) shipped by every Scala 3 library *and the 3.9 stdlib*, (c) independent of JS/Native/JVM backends, and (d) exactly what the reference compiler itself consumes with `-from-tasty`, which gives us a differential-testing oracle (`dotc -from-tasty` output vs our interpretation).
2. **Scripts are compiled to TASTy by the reference compiler**, out-of-process and cached by content hash. This sidesteps the typer entirely for v1 and gives 100% typing fidelity. Add a ~200-line dotc plugin (or `-Yafter-inlining-pickle`-style patch) that **pickles after `PickleQuotes`**, so cached script TASTy is macro-expanded and fully inlined. Libraries from Maven still contain unexpanded `inline` calls, so a *basic* inliner (substitution, `inline if/match` on constants, `constValue`, `Predef.assert`/`summon`, `boundary`) is still needed in the interpreter; treat `summonInline`/match-type reduction as unsupported initially and measure how often the Scala Native test suite hits it.
3. **Do not start with a source typer.** Revisit only if the cold-start requirement cannot be met by caching (e.g. the very first run of a new script must be < 100 ms with no JVM installed). If revisited, scope it as "typer for scripts that only *use* libraries" and keep the TASTy path for everything else.
4. **Keep NIR as the fallback/experiment for the "JDK surface" problem**: Scala Native's `javalib` (Java stdlib re-implemented in Scala) is compiled to NIR, not to TASTy-with-bodies we can use directly, *unless* we build `javalib` ourselves with `-Xjava-tasty`/normal Scala compilation to get TASTy. Investigate in a follow-up whether Scala Native's `javalib`/`scalalib` sources can be compiled to plain TASTy for our interpreter (they are Scala sources, so in principle yes). That would give the exact Scala-Native subset as TASTy.
5. **Macro support**: full via path 2; in a JVM-less environment fall back to "inline-only" and report "macro X requires the Scala compiler to expand"; consider option B (own `Quotes` impl) later for a whitelist of common macro idioms if demand exists.
6. **JVM bytecode compatibility**: not worth it for the interpreter core; if needed later, a JVM-bytecode *loader* for the erased *precompiled* subset is conceptually separate from the TASTy interpreter and could reuse the same runtime object model (both are erased at runtime).

### 6.2 Open questions for follow-up research

- Exact inventory of `inline`/macro/`Mirror` usage in the Scala Native test suites we plan to validate against.
- Whether Scala Native's `javalib`/`scalalib`/`nativelib` compile to standalone TASTy (they use `@extern`, `unsafe`, intrinsics handled in `nscplugin`).
- Startup micro-benchmarks: size of TASTy for a typical script and for the transitive stdlib closure of `List`/`Map`/`String`; cost of lazy unpickling in Rust.
- Feasibility of the "pickle after inlining" plugin (check `TreePickler` on post-`PickleQuotes` trees: `Hole`/`EXPLICITtpt` are explicitly forbidden in TASTy files; `Inlined` nodes are allowed).

---

## Sources

Scala 3.9 / status

- Scala 3.9 LTS released (official announcement): https://scala-lang.org/news/3.9/
- GitHub release 3.9.0: https://github.com/scala/scala3/releases/tag/3.9.0
- Scala 3.9.0 release thread: https://contributors.scala-lang.org/t/scala-3-9-0-release-thread/7477
- Scala 3.8 released (JDK 17 baseline, stdlib compiled with Scala 3): https://www.scala-lang.org/news/3.8/
- State of the TASTy reader / 2.13 ↔ 3 (Feb 2026): https://www.scala-lang.org/blog/state-of-tasty-reader.html
- Binary compatibility / TASTy version mapping: https://docs.scala-lang.org/scala3/reference/language-versions/binary-compatibility.html
- Capture checking (experimental): https://docs.scala-lang.org/scala3/reference/experimental/cc.html
- endoflife.date Scala: https://endoflife.date/scala

Compiler pipeline (scala/scala3 at tag 3.9.0)

- `Compiler.scala` (phase list): https://github.com/scala/scala3/blob/3.9.0/compiler/src/dotty/tools/dotc/Compiler.scala
- Overall structure doc: https://github.com/scala/scala3/blob/3.9.0/docs/_docs/internals/overall-structure.md (rendered: https://dotty.epfl.ch/docs/internals/overall-structure.html)
- `transform/Pickler.scala`: https://github.com/scala/scala3/blob/3.9.0/compiler/src/dotty/tools/dotc/transform/Pickler.scala
- `transform/Inlining.scala`: https://github.com/scala/scala3/blob/3.9.0/compiler/src/dotty/tools/dotc/transform/Inlining.scala
- `inlines/Inlines.scala` (`needsInlining`, `needsTransparentInlining`): https://github.com/scala/scala3/blob/3.9.0/compiler/src/dotty/tools/dotc/inlines/Inlines.scala
- `typer/Typer.scala` (inlining in `adapt`): https://github.com/scala/scala3/blob/3.9.0/compiler/src/dotty/tools/dotc/typer/Typer.scala
- `fromtasty/TASTYCompiler.scala`, `ReadTasty.scala`: https://github.com/scala/scala3/tree/3.9.0/compiler/src/dotty/tools/dotc/fromtasty
- Scala Native `GenNIR.scala` (phase position): https://github.com/scala-native/scala-native/blob/main/nscplugin/src/main/scala-3/scala/scalanative/nscplugin/GenNIR.scala
- Scala Native NIR docs: https://scala-native.org/en/stable/contrib/nir.html
- Scala.js IR trees: https://github.com/scala-js/scala-js/blob/main/ir/shared/src/main/scala/org/scalajs/ir/Trees.scala

TASTy

- `TastyFormat.scala` (normative spec, versions): https://github.com/scala/scala3/blob/3.9.0/tasty/src/dotty/tools/tasty/TastyFormat.scala
- `core/tasty/TreeUnpickler.scala`: https://github.com/scala/scala3/blob/3.9.0/compiler/src/dotty/tools/dotc/core/tasty/TreeUnpickler.scala
- `core/tasty/TreePickler.scala`: https://github.com/scala/scala3/blob/3.9.0/compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
- TASTy overview guide: https://docs.scala-lang.org/scala3/guides/tasty-overview.html
- TASTy inspection: https://docs.scala-lang.org/scala3/reference/metaprogramming/tasty-inspect.html
- Best-effort compilation: https://github.com/scala/scala3/blob/3.9.0/docs/_docs/internals/best-effort-compilation.md (rendered: https://dotty.epfl.ch/docs/internals/best-effort-compilation.html)
- Pipelined builds PR: https://github.com/scala/scala3/pull/18880 ; contributors thread: https://contributors.scala-lang.org/t/try-out-pipelined-builds-in-scala-3-5-nightly/6627 ; recent issues: https://github.com/scala/scala3/issues/27125 , https://github.com/scala/scala3/issues/27133 , https://github.com/scala/scala3/issues/27139
- Published artifacts inspected locally: `org.scala-lang:scala-library:3.9.0` and `org.scala-lang:scala3-library_3:3.9.0` from repo1.maven.org (coursier cache).

Macros

- Macros reference: https://docs.scala-lang.org/scala3/reference/metaprogramming/macros.html
- Macros guide: https://docs.scala-lang.org/scala3/guides/macros/macros.html
- `quoted/Interpreter.scala`: https://github.com/scala/scala3/blob/3.9.0/compiler/src/dotty/tools/dotc/quoted/Interpreter.scala
- `transform/Splicer.scala`: https://github.com/scala/scala3/blob/3.9.0/compiler/src/dotty/tools/dotc/transform/Splicer.scala
- `core/MacroClassLoader.scala`: https://github.com/scala/scala3/blob/3.9.0/compiler/src/dotty/tools/dotc/core/MacroClassLoader.scala
- `transform/MacroAnnotations.scala`: https://github.com/scala/scala3/blob/3.9.0/compiler/src/dotty/tools/dotc/transform/MacroAnnotations.scala
- `quoted/PickledQuotes.scala`, `QuotePatterns.scala`: https://github.com/scala/scala3/tree/3.9.0/compiler/src/dotty/tools/dotc/quoted
- `QuotesImpl.scala`: https://github.com/scala/scala3/blob/3.9.0/compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
- `library/src/scala/quoted/Quotes.scala`: https://github.com/scala/scala3/blob/3.9.0/library/src/scala/quoted/Quotes.scala
- Old TASTy interpreter prototype: https://github.com/scala/scala3/tree/3.9.0/tests/old-tasty-interpreter-prototype
- Debugging macros (internals): https://github.com/scala/scala3/blob/3.9.0/docs/_docs/internals/debug-macros.md
- GraalVM native-image of the Scala compiler (macro class-loading caveat): https://medium.com/graalvm/compiling-scala-faster-with-graalvm-86c5c0857fa3

Prior work / libraries

- tasty-query: https://github.com/scalacenter/tasty-query (README, `build.sbt`, `reader/tasties/TastyFormat.scala`)
- tasty-query on Scaladex: https://index.scala-lang.org/scalacenter/tasty-query
- Scala Center activity reports mentioning tasty-query / expression evaluator: https://scala.epfl.ch/records/2022-Q2-activity-report.html , https://scala.epfl.ch/records/2023-Q3-activity-report.html
- Dotty internals index (GADTs, periods, contexts): https://github.com/scala/scala3/tree/3.9.0/docs/_docs/internals
