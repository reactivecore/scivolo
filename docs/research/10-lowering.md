# 10 — Lowering typed trees to NIR

Date: 2026-09-30. Claims are [V] verified by reading the code or running it, or [I] inferred.
Paths: `D/` = `ref/scala3/compiler/src/dotty/tools/dotc/` (tag 3.9.0), `T/` = `D/transform/`,
`N/` = `ref/scala-native/nscplugin/src/main/scala-3/scala/scalanative/nscplugin/` (v0.5.12).
ROADMAP phase 3, step 4 (#33). Follows [research 09](09-typer.md), whose typed trees are the input.

**Recommendation in one paragraph.** Port dotty's post-typer phases that the scripting subset
needs as passes over the typer's `tpd` arena, in dotty's order, and nscplugin's NIR generation
without its native-interop parts, into a new crate `crates/lower`; write the result as NIR bytes
that the VM and the script cache read like any compiled NIR. A direct "typed tree to NIR" shortcut
is not an option for the phases that name things: nscplugin names every class, method and field
from the symbol as the phases left it (`N/NirGenName.scala:36,144`, `D/core/SymDenotations.scala:
1851`), and the VM dispatches by exact signature (ADR 0001), so `$anonfun$1`, `$outer`, `x$lzy1`,
`super$m`, bridges, mixin forwarders and `Owner$$x` must come out exactly as dotc makes them.
Phases that only remove sugar may be merged or simplified as long as the NIR they lead to is
the same. Test against the NIR dotc and nscplugin produce for the same sources, compared
structurally [I].

## 1. The contract with the typer

Whatever `-Ystop-after:typer` shows is the typer's; everything later is the lowering's (agreed with
the typer's work, #32) [V]. So the typer delivers `@main` proxies, `Inlined` expansions,
eta-expansion, SAM closures, default-argument calls, implicit arguments and `apply`/`unapply`; the
lowering does PostTyper's synthetic members, string interpolators, pattern matching, erasure and
everything after. Input: one `tpd` arena per unit (`crates/typer/src/tpd`, dotty's node classes
and field order), each node with `tpe` (unwidened) and `sym`; synthetic trees marked by synthetic
spans; no `Type::Var` left (zonked). Script symbols and types live in the `tasty::Universe`, so a
pass renames or adds symbols there (`define`, `set_info`, `set_parents`, 08 §6) [V].

## 2. From typer to NIR: dotty's phases

nscplugin adds three phases: `PrepNativeInterop` (after PostTyper), `PostInlineNativeInterop` (after
Inlining) and `GenNIR` after `MoveStatics`, before `GenBCode` (`N/ScalaNativePlugin.scala:56`,
`N/GenNIR.scala:15`); it disables no dotty phase [V]. So GenNIR sees the trees after the whole
transform pipeline (`D/Compiler.scala:42-146`). What the subset needs, by MegaPhase [V]:

| group | phases (lines) | why |
|---|---|---|
| PostTyper | PostTyper 864, SuperAccessors 229, SyntheticMembers 740 | case class and enum members, `super$m` |
| 62-73 | FirstTransform 241, ElimPackagePrefixes 37, BetaReduce 184, ExpandSAMs 240, ElimRepeated 302, DropForMap 149 | canonical trees, `T*` as `Seq`, PartialFunction classes |
| 75-84 | ElimByName 167, HoistSuperArgs 249, TryCatchPatterns 104, PatternMatcher 1,156 | by-name as `Function0`, `$superArg$N`, matches as labelled blocks |
| 91-96 | ExplicitOuter 476, StringInterpolatorOpt 224 | `$outer`, `s""`/`f""` as concatenation |
| 97-113 | SeqLiterals 44, InterceptedMethods 82, Getters 125, SpecializeFunctions 128, SpecializeTuples 52, CollectNullableFields 110, ElimOuterSelect 38, ResolveSuper 128, ParamForwarding 86, LetOverApply 37, ArrayConstructors 59 | getters, `apply$mcII$sp`, super bodies |
| Erasure | Erasure 1,068 with Bridges 192, TypeTestsCasts 406 | erased types, bridges, `$adapted` |
| 115-128 | PureStats 35, EtaReduce 77, ArrayApply 115, TailRec 524, Mixin 378 + MixinOps 130, LazyVals 706, Memoize 189, NonLocalReturns 106, CapturedVars 128 | `$init$`, forwarders, `x$lzy1`, loops for every eligible final method (`T/TailRec.scala:120`) |
| 129-146 | Constructors 380, LambdaLift 351 + Dependencies 327, ElimStaticThis 43, CountOuterAccesses 57, DropOuterAccessors 87, Flatten 68, TransformWildcards 34, ExpandPrivate 127, SelectStatic 99 | ctor bodies, lifted `$anonfun$N`, `C$1`, `Outer$Inner`, `Owner$$x` |

About 11,900 Scala lines, 1,833 of them in PostTyper and its helpers; rarely needed ones
(ProtectedAccessors, ExplicitSelf, TupleOptimizations, value-class ExtensionMethods, …) add 1,175
[V]. Not needed: Pickler, Inlining (the typer expands inline calls), quotes, Scala.js, coverage,
capture checking, the checks (RefChecks, init, `-W`), MoveStatics, CompleteJavaEnums [V].

Phases that only remove sugar and leave no names (PatternMatcher's labels, TailRec's loops,
TryCatchPatterns, SeqLiterals, LetOverApply, …) still decide which calls and branches the NIR
has; they are ported as they are, so the oracle can compare bodies, not merely signatures [I].

## 3. NIR generation (nscplugin)

GenNIR accepts 18 node kinds (Apply, Assign, Block, Closure, Labeled, Ident, If,
JavaSeqLiteral, Literal, Match, Return, Select, This, Try, Typed, TypeApply, ValDef, WhileDo;
`N/NirGenExpr.scala:57-89`) and writes one `.nir` file per top-level name
(`N/NirCodeGen.scala:88-168`) [V]. 7,141 lines; the scripting subset leaves out `@extern`,
`CFuncPtr`, raw pointers and unsigned ops, `@struct`, linktime conditions, native exports and
reflective instantiation, about 2,600 lines, and the three interop phases (`PrepNativeInterop` 297,
`PostInlineNativeInterop` 149) [V]. `AdaptLazyVals` (248) is no phase but a helper GenNIR calls: it
retargets dotty's `LazyVals`/`VarHandle` calls to Scala Native's runtime; the port emits those calls
directly [V].

What it emits [V]:
- **Classes.** `Class`, `Trait` or `Module`; parent `java.lang.Object` for none; traits = direct
  trait parents; `Var` per field in declaration order (the VM's instance layout); `Define` per
  method, `Declare` without a body; interface bridges when an override's erased signature differs
  (`N/NirGenStat.scala:869`); static forwarders in a companion class, and a mirror class for a
  top-level object without one (`:903-939`), which carries the `main` the VM starts from.
- **Closures.** nscplugin makes the lambda classes itself, `Encl$$Lambda$N` with a counter per
  class (`N/NirGenExpr.scala:247`): captures as fields, one method per SAM signature calling the
  lifted `$anonfun` [V].
- **Bodies.** An entry label with the parameters, `Var` slots for assigned locals, one fresh counter
  for locals and labels, `Unwind` on every instruction inside a `try`, a fresh copy of the finally
  block per exit, `Switch` for `Int` matches and `if`-chains otherwise, `StringBuilder` for
  concatenation chains, `BoxesRunTime.equals` where both sides may be boxed [V].

Must be exact for the VM: global names and signatures (dispatch is by exact signature,
`crates/vm/src/interp/dispatch.rs:65`), class shapes and trait order, field order, one file per
top-level name, the `main` forwarder, the module and static initializers (`RE`, `IE`), the entry
label, unique ids per method, correct `Unwind`s [V]. Free: local and label numbering, block
order, `Var`/`Copy` choices (the VM aliases them away), debug names, positions, `InlineHint`,
finally duplication, dead blocks [V]. The oracle compares the exact parts strictly and the free
ones up to renumbering (§5).

## 4. Port strategy and representation

- **Crate.** `crates/lower`: `phases/` (one module per dotty phase or MegaPhase group, sequential
  passes over the arena), `gen/` (NIR generation: classes, expressions, names, types), `write.rs`
  (NIR serialization, a port of Scala Native's `BinarySerializer`, 755 lines). Rust lines about
  12–16k [I].
- **Trees.** The passes rewrite the `tpd` arena in place or into a fresh arena per phase group
  (dotty's `cpy` copies); a pass that needs types after erasure keeps erased types in the same
  interner (`tasty::Erased` gives the classes; a `Type` form of erased types is still to add) [I].
- **Faithful where names come from.** ExplicitOuter, Getters, Erasure (with Bridges),
  Mixin, LazyVals, LambdaLift, Flatten, ExpandPrivate, HoistSuperArgs, SuperAccessors and
  ResolveSuper, Specialize*, ElimByName and ElimRepeated are ported line by line; `crates/tasty`
  already computes the erasure and bridges the VM checks against (08 §6, 99.9% of the standard
  library's signatures) [V].
- **Output.** NIR bytes, one file per top-level name, into the script's cache entry (09 §5 already
  puts the own frontend's NIR there) and from there into the VM's classpath as today. Writing bytes
  keeps one path into the VM (file index, member tables, lazy decoding) and makes the cache and the
  oracle byte-comparable; handing decoded `Defn`s to the VM (`World::define` exists for synthesized
  bridges) would need a second path for member lookup and could not be cached [I].
- **Time.** The budget in 09 §5 gives the lowering about 5 ms for `moderate.scala` [I]; the passes
  touch only the script's trees, not the library's.

## 5. Oracle

Reference: the NIR dotc and nscplugin make for the same sources (`scala-cli --power compile
--native`, as `scripts/programs.py` does), our NIR from the typer's trees for the same sources [V].
Three levels:

1. **Files and headers, exact.** The same set of `.nir` files; per file the same definitions with
   kind, name, signature, parents, attributes and field order. Catches every naming, signature,
   bridge, forwarder and lambda-class difference.
2. **Bodies, up to renumbering.** Locals and labels renumbered in order of first appearance from the
   entry block in depth-first order, then compared. `programs.py` today sorts the lines of each
   definition, which would miss a jump to the wrong block; the lowering needs the stronger form.
   A relaxed mode drops debug names, `Copy`/`Var` differences and finally duplication where we
   choose differently on purpose.
3. **Behaviour.** Run our NIR in the VM and compare with the committed `.check` output.

Corpus: the 14 `tests/programs` (committed nscplugin NIR and `.check` files), then dotty's
`tests/run` within the typer's subset (M4's harness compiles them to NIR already), then the typer's
`tests/pos` pass set compiled to NIR [I].

## 6. Order of work and sizes

Stacked MRs, each gated by the oracle (Rust lines, [I]):
1. Skeleton (~2k): `crates/lower`, the NIR writer, class and method headers, names and types,
   `main` forwarders; oracle levels 1 and 3 for `hello`.
2. Expressions (~3k): calls, `new`, arithmetic, boxing, equality, strings, arrays, `if`/`while`,
   `try`/`catch`/`finally`, labelled blocks; oracle level 2.
3. Classes (~3k): Getters, Memoize, Constructors, ExplicitOuter, Mixin, ResolveSuper,
   SuperAccessors, ParamForwarding, Flatten, ExpandPrivate, HoistSuperArgs.
4. Erasure (~2k): erased types, casts and type tests, bridges, `$adapted`.
5. Functions (~2k): ElimByName, ElimRepeated, closures and lambda classes, ExpandSAMs, LambdaLift,
   CapturedVars, NonLocalReturns, TailRec, Specialize*.
6. Patterns and data (~2.5k): PatternMatcher, TryCatchPatterns, SyntheticMembers, enums, LazyVals,
   StringInterpolatorOpt.
7. Hardening (~1k): `moderate.scala`, M4's subset, the relaxed oracle mode where justified.

## 7. Risks and open questions

- **Erased types in the arena.** dotty keeps erased types as ordinary types after Erasure; our
  `Type` has no erased form yet (`Erased` is a separate value). Either passes after Erasure carry
  `Erased` beside `tpe`, or `Type` gains erased class and array forms [I].
- **Fresh names.** `$anonfun$N`, `C$N`, `x$lzyN` numbers depend on dotty's per-unit fresh-name
  counters and traversal order; a different order renumbers them, and the oracle sees it [I].
- **Phase boundaries.** MegaPhase fusion runs several phases per node in one traversal; porting
  them as separate passes changes nothing observable as long as each phase's pre- and
  postconditions hold, which dotty checks with `-Ycheck`; the port relies on the oracle instead [I].
- **The library's inline bodies.** The typer expands them (09 §5 MR 7); the lowering then sees
  their trees like the script's own, so `Inlined` nodes must survive until FirstTransform drops them
  [I].
- **Open:** whether SyntheticMembers belongs to the typer's side after all (PostTyper runs before
  Pickler; `-Ystop-after:typer` does not show it) — the contract in §1 says lowering.
