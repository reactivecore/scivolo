# 10 — Lowering typed trees to NIR

Date: 2026-09-30. Claims are [V] verified by reading the code or running it, [I] inferred, or [A]
agreed with the typer's work (#32). Paths: `D/` = `ref/scala3/compiler/src/dotty/tools/dotc/` (tag
3.9.0), `T/` = `D/transform/`, `N/` = `ref/scala-native/nscplugin/src/main/scala-3/scala/
scalanative/nscplugin/` (v0.5.12). ROADMAP phase 3, step 4 (#33). Input: the typed trees of
[research 09](09-typer.md).

**Recommendation.** A new crate `crates/lower` ports the post-typer phases the scripting subset
needs, faithfully and as sequential passes in dotty's order, and nscplugin's NIR generation without
native interop; it writes NIR bytes. Both are proposals, to become ADRs once accepted. Why faithful:
nscplugin names classes, methods and fields from the symbols as the phases left them
(`N/NirGenName.scala:36,144`, `D/core/SymDenotations.scala:1851`), and even phases that only remove
sugar add or drop closures (BetaReduce, EtaReduce, DropForMap, ExpandSAMs) and so renumber
`$anonfun$N` and `$$Lambda$N` and change the set of `.nir` files [V]. dotty's miniphases give the
same result run fused or one after another, so separate passes are sound [V]. Why bytes: one path
into the VM, whose member lookup reads the file index (`World::define` adds a definition but not
its owner's member list) [V], and the script cache stores files anyway (09 §5).

## 1. The contract with the typer

Whatever `-Ystop-after:typer` shows is the typer's; everything later is the lowering's [A]. The typer
delivers `@main` proxies, `Inlined` expansions, eta-expansion, SAM closures, default-argument calls,
implicit arguments, `apply`/`unapply`; desugaring already made enums' `$values`, `fromOrdinal` and
`$new` and the default getters (07 §5, `crates/syntax/src/desugar/defaults.rs`) [V]. Input: one `tpd`
arena per unit, nodes with unwidened `tpe` and `sym`, synthetic spans, no `Type::Var` left; script
symbols live in the `tasty::Universe` [A]. The unit's fresh-name counters go along: they are per unit
and per prefix (`D/core/NameKinds.scala:221`, `compilationUnit.freshNames`), the typer names closures
`$anonfun`, and LambdaLift freshens them again (`T/LambdaLift.scala:66-71`), so the numbers depend on
both [V]; `typer::Typed` hands the unit's `syntax::names::FreshNames` (shared by the parser and
desugaring) on beside the arena, and the lowering counts on from there [A]. Structural (`Selectable`) calls fall back (09 §5).

## 2. From typer to NIR: dotty's phases

nscplugin adds `PrepNativeInterop` (after PostTyper), `PostInlineNativeInterop` (after Inlining) and
`GenNIR` after MoveStatics, before GenBCode (`N/ScalaNativePlugin.scala:56`, `N/GenNIR.scala:15`),
and disables no dotty phase; GenNIR sees the trees after the whole pipeline (`D/Compiler.scala:42-146`)
[V]. What the subset needs, by `List(...)` group of `Compiler.scala` (lines; `L/` = `T/localopt/`):

| group | phases | adds |
|---|---|---|
| 42 | PostTyper 864, SuperAccessors 229, SyntheticMembers 740 | case class and enum members, `super$m` |
| 62-73 | FirstTransform 241, ElimPackagePrefixes 37, BetaReduce 184, ExpandSAMs 240, ElimRepeated 302, `L/`DropForMap 149 | PartialFunction classes, `T*` as `Seq` |
| 75-84 | ExtensionMethods 219 + FullParameterization 269, UncacheGivenAliases 65, ElimByName 167, HoistSuperArgs 249, TryCatchPatterns 104, PatternMatcher 1,156 | `m$extension`, given aliases as `def`s, `Function0`, `B$superArg$1`, labelled blocks |
| 91-96 | ElimOpaque 75, ExplicitOuter 476, `L/`StringInterpolatorOpt 224 + `L/`FormatChecker 323, DropBreaks 262 | `$outer`, `s""`/`f""`, local `boundary.break` |
| 97-113 | InlinePatterns 74, VCInlineMethods 120, SeqLiterals 44, InterceptedMethods 82, Getters 125, SpecializeFunctions 128, SpecializeTuples 52, CollectNullableFields 110, ElimOuterSelect 38, ResolveSuper 128, ParamForwarding 86, LetOverApply 37, ArrayConstructors 59 | value-class calls as `$extension` calls, getters, `apply$mcII$sp` |
| 114 | Erasure 1,068, Bridges 192, TypeTestsCasts 406 | erased types, bridges, `$adapted`; drops `Inlined` (`T/Erasure.scala:875`) |
| 115-128 | ElimErasedValueType 143, PureStats 35, VCElideAllocations 55, EtaReduce 77, ArrayApply 115, TailRec 524, Mixin 378 + MixinOps 130, LazyVals 706, Memoize 189, NonLocalReturns 106, CapturedVars 128 | `$init$`, forwarders, `x$lzy1`, loops for every eligible final method (`T/TailRec.scala:120`) |
| 129 | Constructors 380 | primary constructor bodies |
| 132-135 | LambdaLift 351 + Dependencies 327, ElimStaticThis 43, CountOuterAccesses 57 | `owner$$anonfun$1`, `C$1` |
| 136-146 | DropOuterAccessors 87, Flatten 68, TransformWildcards 34, ExpandPrivate 127, SelectStatic 99 | `Outer$Inner`, `Owner$$x` |

About 13,500 Scala lines [V]. Value classes are not rare: RichInt, StringOps, ArrayOps and
ArrowAssoc are library value classes, so `1 to 10`, `f"…"` and `array.toList` go through them; the
committed `moderate$package$.nir` calls `format$extension` [V]. Not needed: Pickler, Inlining (the
typer expands inline calls), quotes, Scala.js, coverage, capture checking, the checks (RefChecks,
init, `-W`), MoveStatics, CompleteJavaEnums [V]. `Inlined` nodes stay in the trees until Erasure;
PostTyper, BetaReduce, DropForMap and DropBreaks see them (`T/PostTyper.scala:634`) [V].

**Phase-versioned infos.** dotty's phases change symbol infos as they go (DenotTransformers), and
later code reads the info as of an earlier phase: `atPhase` in Mixin (8 uses), ExplicitOuter (5),
Erasure (5), CapturedVars (4), and GenNIR itself (`atPhase(flattenPhase)` for mirror classes)
[V]. Setting infos in place in the `Universe` (`set_info`) loses the earlier view; the port needs
an info per symbol and phase for the phases that read back, at least "before erasure" and "after
erasure" [I].

## 3. NIR generation (nscplugin)

GenNIR accepts 18 node kinds (`N/NirGenExpr.scala:57-89`) and writes one `.nir` file per top-level
name (`N/NirCodeGen.scala:88-168`) [V]. Of its 7,141 lines, the subset leaves out `@extern`,
`CFuncPtr`, raw pointers and unsigned ops, `@struct`, linktime conditions, native exports and
reflective instantiation, and the two interop phases (`PrepNativeInterop` 297,
`PostInlineNativeInterop` 149); `AdaptLazyVals` (248) is a helper GenNIR calls to retarget dotty's
`LazyVals`/`VarHandle` calls to Scala Native's runtime, and the port emits those calls directly [V].

What it emits [V]: `Class`, `Trait` or `Module` with parent, trait parents, a `Var` per field in
declaration order and a `Define` per method (`Declare` without a body); interface bridges where an
override's erased signature differs (`N/NirGenStat.scala:869`); static forwarders in a companion
class, and a mirror class for a top-level object without one (`:903-939`), which holds the `main`
the VM starts from; one class per closure, `Encl$$Lambda$N` with a counter per class
(`N/NirGenExpr.scala:249`), fields for the captures and one method per SAM signature calling the
lifted `$anonfun`. Method bodies: an entry label with the parameters, `Var` slots for assigned
locals, one fresh counter for locals and labels, `Unwind` on every instruction inside a `try`, a
copy of the finally block per exit, `Switch` for `Int` matches, `StringBuilder` for concatenation
chains, `BoxesRunTime.equals` where both sides may be boxed.

What the VM relies on [V]: global names and signatures, class shapes and trait order, field order,
one file per top-level name, the `main` forwarder, `RE`/`IE` initializers, the entry label, unique
ids per method, correct `Unwind`s. Dispatch looks a signature up exactly but falls back to bridges it
builds on demand (`crates/vm/src/interp/bridge/mod.rs:46`, ADR 0008), so a missing bridge still runs;
exactness is the oracle's demand, not the VM's. Local numbering, block order, `Var`/`Copy` choices,
debug names, positions and finally duplication do not matter to the VM [I].

## 4. Port

- **Crate.** `crates/lower`: `phases/` (a module per phase or group, sequential passes over the
  arena, with dotty's tree copier and `TreeTypeMap` for LambdaLift, TailRec and Constructors),
  `gen/` (classes, expressions, names, types), and a NIR writer in `crates/nir` (a port of Scala
  Native's `BinarySerializer`, 755 lines; `crates/nir` only decodes today) [V].
- **Erased types.** After Erasure dotty's trees carry erased types as ordinary types; `tasty::Erased`
  is a separate value, so `Type` needs erased class and array forms, or the passes after Erasure
  carry `Erased` beside `tpe` [I].
- **Symbols.** Renames and new symbols (`$outer`, getters, lifted methods, lambda classes) go into the
  `Universe` through `define`; infos per phase as in §2 [I].
- **Faithful where it matters.** Everything in the table is ported line by line; `crates/tasty`
  already computes erasure and bridges (99.9% of javalib's and 99.8% of the standard library's
  signatures match NIR, ROADMAP phase 3 step 2) [V].

## 5. Oracle

Reference: the NIR dotc and nscplugin make for the same sources (`scala-cli --power compile
--native`, as `scripts/programs.py` does) [V]. Its bytes are not stable (positions, paths:
`programs.py:98-99` compares decoded definitions), so the oracle compares decoded NIR at three
levels [V]:

1. **Files and headers, exact.** The same `.nir` files; per file the same definitions with kind,
   name, signature, parents, attributes and field order.
2. **Bodies, up to renumbering.** Locals and labels renumbered by first appearance in depth-first
   order from the entry block. `programs.py` sorts each definition's lines, which misses a jump to the
   wrong block. A relaxed mode ignores debug names, `Copy`/`Var` and finally duplication.
3. **Behaviour.** The VM runs our NIR; the output must equal the committed `.check`.

Corpus: the 14 `tests/programs` (committed NIR and `.check`), dotty's `tests/run` within the typer's
subset (M4's harness already compiles them), the typer's `tests/pos` pass set [I].

## 6. Order of work and sizes

About 18k Scala lines (13.5k phases, 4.5k GenNIR, 0.75k serializer) plus tree copiers and
phase-versioned infos; 17–22k Rust lines, compared with 09's 24.8k Scala to 15–19k Rust [I].
Stacked MRs, each gated by the oracle:
1. Skeleton (~2.5k): writer, class and method headers, names, types, `main` forwarders, calls and
   literals; `hello` at levels 1 and 3.
2. Erasure and expressions (~4k): phase-versioned infos, erased types, Erasure with bridges and
   casts, value-class phases, boxing, equality, strings, arrays, control flow, `try`; level 2.
3. Classes (~3.5k): Getters, Memoize, Constructors, ExplicitOuter, Mixin, ResolveSuper,
   SuperAccessors, ParamForwarding, Flatten, ExpandPrivate, HoistSuperArgs.
4. Functions (~3k): ElimByName, ElimRepeated, closures and lambda classes, ExpandSAMs, LambdaLift,
   CapturedVars, NonLocalReturns, TailRec, DropBreaks, Specialize*.
5. Patterns and data (~3.5k): PatternMatcher, TryCatchPatterns, SyntheticMembers, LazyVals,
   UncacheGivenAliases, string interpolators with FormatChecker.
6. Hardening (~1.5k): `moderate.scala`, M4's subset, the relaxed oracle mode where justified.

## 7. Risks

- **PatternMatcher follows the typer.** Its plan depends on the scrutinee's type, the extractor form
  (product, `get`, `unapplySeq`, name-based), irrefutability from `unapply` result types and type
  tests elided when the static type conforms; any typer difference shows as different labels and
  tests at oracle level 2 [I].
- **Fresh names** depend on the counters the typer hands over and on the traversal order of every
  phase that names things (§1) [I].
- **Phase-versioned infos** (§2) are new design; getting them wrong shows as wrong erased signatures
  or missing outer accessors [I].
