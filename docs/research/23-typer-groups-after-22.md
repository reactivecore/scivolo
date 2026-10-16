# 23 — Typer and lowering groups after research 22

Date: 2026-10-16. Claims are [V] verified by reading the code or running it, [I] inferred. Paths:
`T/` = `ref/scala3/compiler/src/dotty/tools/dotc/typer/` (3.9.0), `C/` = `.../dotc/core/`, `X/` =
`.../dotc/transform/`, `N/` = `.../dotc/inlines/`, `Y/` = `crates/typer/src/typer/`, `Yn/` =
`crates/typer/src/namer/`, `L/` = `crates/lower/src/`, `S/` = `crates/tasty/src/`. Measured with
`just dotty-lower-report` on the top of today's MR stack (`83-typer-group-a-rest` 9af7b68f:
lowering gaps, exports, match types, access, avoidance, trait parameters, implicits): **943 pass**
(878 on main c969c399, 65 new, none lost), 480 typer refusals, 8 lowering refusals, 1 crash
(i15618) [V]. Every test named below was run through `just typer-dump` (`tmp/r23/probe.py`) to
find its refusal site and line [V]. Follows [research 22](22-typer-exports-trait-params.md).

**Recommendation.** The research 22 groups are gone from the top of the table (exports, trait
parameters, access, `Dynamic` is #42 in work) but for dependent results (§2.4). What remains
splits into four real ports — mirrors (§2.1, ~30 tests, #43), inline leftovers (§2.2, 29 tests,
#82), type patterns of abstract types (§2.3, 27 tests) and structural types with reflective calls
(§2.8, 9 tests, typer + lowering + VM) — and a dozen one-day items of 4–10 tests each (§2.5–§2.7,
§2.9). Eleven MRs, ~2,700 lines, about 140 of the 480 refused tests (§3); the rest are macros,
named tuples, `typeChecks`, JDK-only API and singles.

## 1. The refusals

First refusals of `just dotty-lower-report` with their site [V]; "Dynamic" (10) is #42 and left out.

| run | reason | site | § |
|---:|---|---|---|
| 26 | no implicit argument | `Y/implicits/search.rs:81,118`; 17 are `summon[Mirror.*Of[T]]` | 2.1 |
| 22 | adaptation | `Y/adapt.rs:111-112`; 7 are dependent results (§2.4), the rest singles | 2.4, 2.9 |
| 22 | member not found | `Y/select.rs:82-83,92`; 4 `m.MirroredElemTypes` (§2.1), 9 `mac"$x"` macros (#82) | 2.1 |
| 18 | deferred, given or overriding inline definition | `Yn/checks.rs:104-105` | 2.2 |
| 17 | type pattern needing a type test or type binding | `Y/patterns.rs:34-35` (`is_testable_type` `:111-119`) | 2.3 |
| 16 | Tuple | `Y/dispatch.rs:76-79` (a `Tree::Tuple` of more than 22 elements or with names) | 2.7 |
| 11 | type tree Tuple | `Y/type_trees.rs:108-111` (same for types) | 2.7 |
| 11 | non-public member in an inline method | `Y/inline_refs.rs:46-47` (`needs_inline_accessor` `:54-71`) | 2.2 |
| 9 | overloaded reference with type arguments | `Y/alternatives.rs:81-95` (`Pt::PolyFun`) | 2.5 |
| 8 | opaque alias other than a class | `Y/type_defs.rs:54-57` | 2.6 |
| 8 | source versions other than 3.9 | `Y/mod.rs:231-232` | 2.9 |
| 8 | scala.compiletime operation | `Y/inline_refs.rs:35-41`: all `typeChecks`/`typeCheckErrors`, out | — |
| 7 | application of a non-method | `Y/apply.rs:64`, `Y/apply_insertion.rs:27` | 2.9 |
| 7 | by-name function parameter | `Y/function_types.rs:138-139` | 2.9 |
| 6 | annotated type | `Y/type_trees.rs:94-95` | 2.9 |
| 6 | no applicable alternative | `Y/overloads.rs:102` | 2.5 |
| 6 | named or default arguments of a repeated method, … | `Y/named_args.rs:41` | 2.9 |
| 6 | derives clause | `Y/classes.rs:46` | 2.1 |
| 6 | extractor of a type that needs a type test | `Y/unapply.rs:67-68` | 2.3 |
| 6 | wrong number of type arguments | `Y/type_args.rs:21`, `Y/type_trees.rs:25`; 3 `throws`, out | — |
| 6 | default argument | `Y/default_args.rs:33-34,47` | 2.9 |
| 6 | refinement of an anonymous parent | `Y/refined_types.rs:19` | 2.8 |
| 5 | dependent result type | `Y/apply.rs:130-134` (`dependent_result`, `Y/dependent_params.rs:33-58`) | 2.4 |
| 5 | sequence argument of an array or a bottom type | `Y/repeated.rs:110-117` | 2.9 |
| 5 | type tree PolyFunction | `Y/type_trees.rs:108-111` | 2.9 |
| 5+4 | type lambda other than …; higher-kinded type lambda parameter | `Y/type_lambdas.rs:16,34,58` | research 19 G2 |
| 4 | derived type without a source | `Y/derived.rs:21` | 2.9 |
| 4 | pattern-bound type variable | `Y/unapply.rs:193` | 2.3 |
| 4 | union widening | `Y/widen_union.rs:54-55` (`S/subtype/join.rs:9`) | 2.9 |
| 4 | block type with local symbols | `Y/stats.rs:147` (research 22 B leftovers: i13703, i14340, tcpoly_monads, selectable-new) | 2.8, #122 |

Lowering (8, §2.10): reflective calls 2 (`L/codegen/callee.rs:34-38`), and one each at
`L/phases/outer_fields.rs:95`, `nested_reads.rs:54`, `context_results.rs:41`, `local_traits.rs:43`,
`outer_args.rs:32`, `L/codegen/statics.rs:77` [V].

## 2. Causes

### 2.1 Mirrors (#43: 17 + 4 + 6 + 7 tests)

`summon[Mirror.Of[T]]`, `Mirror.ProductOf`, `Mirror.SumOf` for a case class, a sealed trait, an
enum or a case object fail the implicit search (anon-mirror-gen-local, deriving,
deriving-from-product-typed, i10997, i11050, i12328a/b, i12919a/b, i13332shapeless-b,
i14432/a/b/c, i15190, scala_enum_testing, unroll-multiple) [V]; `m.MirroredElemTypes` on a
`using m: Mirror.ProductOf[A]` is "member not found" (deriving-constructor-order, i23897, i7049,
poly-kinded-derives) [V]; `derives` clauses are refused at `Y/classes.rs:46` (6) [V]; and 7 of
the 18 retained inline givens of §2.2 are `inline given derived[T](using m: Mirror.Of[T])`.

dotty: `inferImplicitArg` falls back to `Synthesizer.tryAll` (`T/Implicits.scala:948-955`,
`T/Synthesizer.scala:813-833`), a list of `specialHandlers` (`:798-811`) keyed by class:
`ClassTag`, `TypeTest`, `CanEqual`, `ValueOf`, `TupledFunction`, `Mirror.Product`, `Mirror.Sum`,
`Mirror`, `Manifest`, `Singleton`, `Precise`. `makeMirror` (`:654-664`) reads `MirroredType` from
the formal's refinement; `productMirror` (`:447-535`): `MirrorSource.reduce` (`:377-`) finds the
class and prefix (or a generic/named tuple), `makeClassProductMirror` (`:474-483`) takes the case
accessors' labels and `memberInfo` types, `mirrorRef` is the companion when
`useCompanionAsProductMirror` (`C/SymUtils.scala:166`), else `anonymousMirror` (`:277`: an
anonymous class with `fromProduct`, the `ExtendsSumOrProductMirror` attachment for
`SyntheticMembers`), and `makeProductMirror` (`:484-500`) builds `Mirror.Product { type
MirroredMonoType; MirroredType; MirroredLabel; MirroredElemTypes = (A, B); MirroredElemLabels =
("a", "b") }` with `mirrorCore` (`:297`) and casts the reference to it; `sumMirror` (`:538-`) the
same for `isGenericSum` (`C/SymUtils.scala:239`) with `ordinal`. `derives` is `T/Deriving.scala`
(328 lines): one `given derived$TC[T...]: TC[C[T...]] = TC.derived` per clause. Lowering:
`X/SyntheticMembers.scala:660` `addMirrorSupport` gives companions `fromProduct` (`:545`) and
`ordinal` (`:627`).

Ours: `Y/implicits/synthesis.rs` synthesizes `CanEqual`, `ClassTag` and tuple mirrors
(`tuple_mirrors.rs`) [V], nothing for classes; the lowering already has `L/phases/mirrors.rs`
(`addMirrorSupport`: companion `fromProduct`, case-object `Mirror.Singleton`) and `sum_mirrors.rs`
(`ordinal`; the former "companions of sealed classes other than enums" refusal is gone) [V], so
the typer half is what is missing. `MirroredElemTypes` of a `using` parameter is a type member of
a refined type `Type::Refined` (research 20 N4) seen through a `TermRef` prefix; `members.rs:80`
follows `Refined` to its parent [V], so the miss is the refinement's own member or the
`Mirror.ProductOf[A]` alias not being dealiased to its `Refined` form [I, probe with i7049].

Port (~450 lines): `Y/implicits/mirrors.rs` (`productMirror`/`sumMirror`: `MirrorSource`, labels,
`Refined` chain with `nested_pairs`, companion ref or an anonymous class through
`Y/anon_classes.rs` with the attachment the lowering's `mirrors.rs` needs for `fromProduct`),
hooked beside `tuple_mirrors.rs` in `synthesis.rs`; member lookup of `MirroredElemTypes` on a
parameter; then `derives` (~150: `Deriving.scala`'s `derived$TC` givens as synthesized `given`
defs in the companion, entered in `Yn/`). Tests: deriving, deriving-from-product-typed, i10997,
i12328a/b, i12919a/b, i14432/a/b/c, i15190, scala_enum_testing, unroll-multiple, i11050,
i13332shapeless-b, anon-mirror-gen-local (17; i12919 after §2.4), i23897/i7049/poly-kinded-derives/
deriving-constructor-order (with the inline matches of #120 M4), the six `derives` tests
(derived-specificity, deriving-interesting-prefixes, i11961, i15101, i9011,
typeclass-derivation3), then the 7 mirror givens of §2.2.

### 2.2 Inline leftovers (#82: 18 + 11 tests)

Retained inline methods: `inline given` (erased-machine-state, i11542, i11563, i12328, i13146,
i13146a, i13332shapeless, i13334, i14150, i9473, typeclass-derivation-doc-example — 11),
`inline override def` (i15725, i17584a, quote-MacroOverride, transparent-foreach,
typelevel-overrides — 5), deferred `inline def` (inline-override, typeclass-derivation2b) stop in
the namer, `Yn/checks.rs:104-105` [V]. dotty keeps such a method callable: `typedDefDef` adds a
`bodyRetainer` (`T/Typer.scala:4162-4164`, `N/Inlines.scala:267-277`: `private def
f$retainedBody(x) = f(x)` with the call inline-expanded, flags without `Inline | Override`) and
erasure moves its body back into `f` (`X/Erasure.scala:106`, `:976-983` `addRetainedInlineBodies`),
dropping other inline methods (`:893`). [Research 12](12-inlining.md) lists this as "later"
(its §1 table and item 7: `bodyRetainer`, `X/Erasure.scala:993`). Ours: the lowering emits no
body for an `INLINE` method (`L/codegen/body.rs:48`), refuses a call to one that was not expanded
(`L/codegen/callee.rs:28`) and keeps it out of static forwarders (`statics.rs:84`) [V]; the
expansion is `crates/typer/src/inlining/`. A retained one needs its body typed *as a method body*
once (the inlined expansion of the self-call) and kept.

Port (~200 lines): `checks.rs:104` keeps only `inline given` with inline parameters and macros
refused; `Y/defs.rs`'s inline branch types `f$retainedBody` as a sibling `DefDef` whose rhs is the
expanded call (`inlining::expand` on `f(params)`), entered as `PRIVATE`; `body.rs:48` emits the
retainer's body under the method's own name and signature (`addRetainedInlineBodies`) and
`callee.rs:28` admits calls to a retained method (virtual calls of an `inline override` reach the
retained body, as in dotty); `inline given` and `inline override` members are then ordinary
members for `Mixin`/`ResolveSuper`. Tests: 5 overrides
(i15725, i17584a, quote-MacroOverride, transparent-foreach, typelevel-overrides), inline-override,
i13334, i14150, erased-machine-state (9; the mirror givens after §2.1, typeclass-derivation2b and
i13332shapeless after #120 M4) [I].

Inline accessors: a private or protected member used in an inline body (i12160 `P.s`, i13215,
i15374, i16252, i22498 `new Foo(0)` of a private constructor, i4754, transparentPrivates,
transparentProtected, transparentByName/transparentForeach `var i`, i11706) is refused at
`Y/inline_refs.rs:46-47` [V]. dotty: `PrepareInlineable.makeInlineable` (`N/PrepareInlineable.scala:36-40`)
rewrites the body with `MakeInlineableDirect` (`:114`, a public `inline$x` accessor in the member's
class when the inline method is inside it) or `MakeInlineablePassing` (`:161`, an accessor taking
the receiver as first argument, `InlineAccessorName(UniqueInlineName)`), `needsAccessor` `:75-88`
(ours `:54-71` is that predicate); `addAccessorDefs` adds them to the class. Port (~180 lines):
`Y/inline_accessors.rs` at the end of `typed_template_stats` — for each reference the inline body
recorded (`needs_inline_accessor` true) define `inline$name` (getter, setter for a `var`, method
with the member's clauses, `new` through a `$lessinit$greater` accessor) with `syms.define`, and
rewrite the body's reference to it so the expansion at the call site is public; the lowering sees
plain methods. Tests: all 11 but i13215 (#121's qualified library privates) and i11706 (`loop` is
`private` in a `transparent inline` recursion — probe) [I].

### 2.3 Type patterns of abstract types (17 + 6 + 4 tests)

`case t: T` with `T` a type parameter (`[T: ClassTag]`, i1099; `E <: Throwable` in a class,
i11914/i11914a, rescue, rescue-boundary; i20225, for-desugar-strawman), an abstract type `Succ`
of a fully abstract `Nat` (fully-abstract-nat-1/3/4/5/6/7, the test *expects* the unchecked
warning), `Matcher[A]` a match type with a `TypeTest[Any, Matcher[A]]` given (i13433/b), an alias
of a class (`ExceptionType`, tryPatternMatch; `CaseDef`, patmatch-classtag) stop at
`Y/patterns.rs:34`: `is_testable_type` wants a class, singleton or union of them [V].
`Succ(_)`/`AppliedOp(_,_,_)` extractors on abstract types (fully-abstract-nat, -nat-2,
-interface, type-test-nat), `Tuple()` (tuples-empty), `ExtractFoo()` (i1991) hit the same test in
`Y/unapply.rs:67-68`, and `Identity(g)`/`Array(i: Long)`/`C(IsInt, i)`/`val X2(a2, b2)` bind a
type variable (`:193`) [V].

dotty: the typer accepts any type pattern; `typedTyped` in pattern mode calls `tryWithTypeTest`
(`T/Typer.scala:1389-1410`): for a non-class `TypeRef` or applied non-class type it searches a
`TypeTest[pt, tpe]`, then a `ClassTag[tpe]`, and rewrites the pattern to `tag(expr)` (an
`UnApply` on the given, `Mode.InTypeTest`); without one the `Typed` pattern stays and erasure's
`TypeTestsCasts.interceptTypeApply` (`X/TypeTestsCasts.scala:185`) emits the erased test
(`transformTypeTest` `:320-`, singletons, `|`/`&`, generic arrays, `Tuple`) and warns
`UncheckedTypePattern` through `checkTypePattern` (`:386-393`, `whyUncheckable`) unless the
scrutinee is `@unchecked`. `isCheckable`/`checkSensical` (`:201-240`) decide "will always fail".

Port (~250 lines): typer ~120 — `patterns.rs:34` becomes `try_with_type_test`: `TypeTest`/
`ClassTag` search through `Y/implicits/` (synthesized `ClassTag`s exist, `class_tags.rs`; a
synthesized `TypeTest` is `Synthesizer.synthesizedTypeTest`, `T/Synthesizer.scala:95`, ~30 lines:
`TypeTest` of a class type from its `ClassTag`), the pattern typed as an extractor on the given (i1099's `foo2` shows the
expansion); without a given the `Typed` stays with the warning text of `UncheckedTypePattern`
(`just typer-compare` has the oracle's warnings); `unapply.rs:67` the same for the extractor's
scrutinee; `:193` binds the type variable as a `TypeParam` symbol with the inferred bounds (dotty
`typedBind` on a type, `Inferencing.constrainPatternType`, research 19 "GADT out" — the four
tests only need the bound, not GADT reasoning [I]). Lowering ~80 — `L/phases/type_tests/`
`runtime_tests.rs` tests the erased class of an abstract type (its erasure is the bound's,
`S/erasure`), a `TypeTest` application is an ordinary `unapply` call. Tests: i1099, i11914,
i11914a, rescue, rescue-boundary, i20225, tryPatternMatch, patmatch-classtag, fully-abstract-nat,
-nat-1…7, -interface, type-test-nat, i13433/b (with #120's `TypeTest[Any, Matcher[A]]` given —
Matcher reduces), tuples-empty, i1991, for-desugar-strawman (~24); enum-approx, i14693, i5077,
tuple-accessors after the type-variable binding [I].

### 2.4 Dependent results (research 22 D: 5 + 7 tests)

`dependent_result` (`Y/dependent_params.rs:33-58`) substitutes only when *every* argument is a
stable path and no inner-class type mentions a parameter; otherwise `apply.rs:134` refuses [V].
Stopped there: `at(ys2, Succ(Zero))` (HLists, HLists-nonvariant: an unstable argument whose type
is `Succ[Zero]`), `addString(new StringBuilder, "_")` (t5656), `instantiate[Normal](Tuple1("a"))`
(i12919), `concat0(Tuple(), tl1)` (tuples1). The seven adaptation refusals of the same cause [V
site, I cause]: `lazyDep(X)(foo)` (t6443-by-name, t6443-varargs: a second clause depending on the
first), `y(2)(3)` (given-eta: eta-expansion of `def g(x: Int)(using d: D)(y: d.T): d.T`),
`true.:#:("hello")(23)` (i11583), `dh.Out` (byname-implicits-6), `foo.Bar(5)` as a default
(i4600b), `(x.show, x.eval)` (i9530). dotty substitutes unconditionally with `safeSubstParams`
(`T/TypeAssigner.scala:294-300`), skolemizing unstable arguments (`SkolemType(argType.widen)`),
and `Applications.scala:834-845` substitutes typed arguments into later formals — research 22
§2.2 has the detail; `applied_result` (`dependent_params.rs:70-`) is already `safeSubstParams`
with `fresh_skolem` [V].

Port (~120 lines): `dependent_result` drops the stability test (`fresh_skolem` for the unstable
ones; the inner-class guard stays until #122's skolem prefixes print as dotty does), the
`.filter(|_| plain)` goes (lifted and aligned arguments substitute the lifted `val`'s type — a
stable path), D2 of research 22 for parameter-dependent later clauses (`apply.rs:90-96` refuses "dependent
parameter or private type" where a formal mentions a parameter [V]), eta-expansion in `Y/eta.rs`
with the substituted result. Tests: HLists,
HLists-nonvariant, t5656, t6443-by-name, t6443-varargs, given-eta, i11583, byname-implicits-6
(then by-name implicit search), i4600b, i9530 (~9); tuples1 with #120, i12919 with §2.1.

### 2.5 Overloads with type arguments and failed overloaded applications (9 + 6 + 4 tests)

`Array.ofDim[Int](5)` (optimizer-array-load, t0677-old, t2005), `Array.fill[Char](3)('-')`
(t6827), `A.a[String][Int](3)` (i3930), `b.foo[Int].i` (overloading-specifity),
`injectorFactory[String](…)` (i15913), `valueOf[Pattern]`/`valueOf[1]` (i8530-b, sip23-valueof)
stop at `Y/alternatives.rs:91-93`: with `Pt::PolyFun` the chooser wants a single polymorphic
alternative [V]. dotty (`T/Applications.scala:2678-2688`): `PolyProto(targs, pt1)` keeps the
alternatives that `canInstantiate` (`:2501`: a `PolyType` of that arity), drops those whose bounds
the arguments violate, and otherwise resolves `resolveMapped(alts, _.widen.appliedTo(targs), pt1)`
(`:2811`) — overload resolution of the *instantiated* alternatives against the application's
`FunProto` that follows. Ours has no `pt1` inside `Pt::PolyFun`; `typed_type_apply`
(`Y/type_apply.rs`) is called with the outer `Pt::Fun` lost. Port (~90 lines): `Pt::PolyFun`
carries the arguments' type trees and the enclosing proto (as `Pt::Fun(proto)` does), `choose`
filters by arity and bounds and calls `resolve_overloaded` on the instantiated infos
(`Y/overloads.rs` with a `prefix` of mapped types, dotty's `resolveMapped`). Tests: the 9 [I:
sip23-valueof/i8530-b also need `ValueOf` synthesis, `Synthesizer.synthesizedValueOf`
`T/Synthesizer.scala:236`, 20 lines].

"no applicable alternative" (`Y/overloads.rs:102`): `5 + BigDecimal(3)` (bigDecimalTest),
`1 - Seq.empty[Int]` (i16464), `3.5 * Hour` (impconvtimes) — an implicit view on the *receiver*
after every overload of `Int.+` fails (dotty `tryInsertImplicitOnQualifier`, `T/Typer.scala:4278`,
from `typedApply`'s failure handler `T/Applications.scala:1380`; research 20 §2.7 did the
single-method case in `Y/apply.rs`); `decode(v.codec)` (i25000/b, a dependent `v.codec` type) §2.4; `xs(i)` on an
`IArray` (iarrays: opaque `IArray` extension `apply`, §2.6). "overloaded reference" (4):
`foo(f)` with `f` a lambda value against overloads (i4364a, research 19 §2.4 group B),
regex `unapplySeq` extractors (t5045, t6406-regextract: overloaded `unapplySeq` of `Regex`), the
`returning` object [I]. Port (~40 lines) for the three receiver views, the rest stays.

### 2.6 Opaque aliases other than classes (8 tests)

`opaque type Viewers = Set[Person]`, `Time = String`, `Tup = (Int, String)`, `ExitLabel =
Label[Unit]`, `Caller = AbstractSignal[?]`, `Showed = String` stop at `Y/type_defs.rs:54-57`,
which admits only an unapplied class [V] (ConfManagement, i13461-b, i14587.opaques, i9928,
loops-alt, Signals1, Signals2, string-context-implicits-with-conversion). Research 19 §2.6 (E3)
describes the port; the class restriction is what E3 landed with. The pieces around it already
take any alias: `set_opaque_alias` (`S/symbols/opaques.rs:42`) stores the type, erasure follows
it (`S/erasure/mod.rs:121`, `applied.rs:143,181`), the lowering's `L/phases/elim_opaque.rs` maps
every tree type through it and refuses `==` on opaque values [V]; no reason for the class
restriction is recorded (no ADR, no comment) [V], so the first step is a probe of `opaque type T =
String` and `= Set[Person]` through `just lower-run` and `just nir-cmp` to find which pass breaks
(the subtyping's `liftToThis` for an applied alias, or `elim_opaque.rs` on a type with arguments
[I]). Port (~80 lines): `type_defs.rs:55` admits any alias type without type parameters
(`type_arity == 0` stays for HK opaques, i18638 out); whatever the probe shows; a `tests/programs`
entry with a `String`, a `Set[…]` and a tuple alias. Tests: i13461-b (then #82),
i14587.opaques, i9928, string-context-implicits-with-conversion (then views), ConfManagement,
Signals1, Signals2, loops-alt (with `?=>` A5, landed) — 5–8 [I].

### 2.7 Tuples: XXL and named (16 + 11 tests; #120)

Expressions of more than 22 elements (`val t: Tuple = (1, …, 25)`: i8314, tuple-concat, -cons,
-erased, -map, -patmat, -patmat-extract, -patmat-size, -patterns, -split, -tail, -zip — 12) and
named tuples (`(name = "Bob")`, i16213, i23131, i23440, named-patmatch) fall to the dispatcher's
catch-all (`Y/dispatch.rs:76-79`), which has no `Tree::Tuple` arm: small tuples reach the typer
as `TupleN` applications already [V site, I for where they are built]. The same for types
(`type_trees.rs:108-111`): `(Int, …×23)` (i5260, i15988a, i15988b) and named tuple types
`(name: String, age: Int)` (bind-tuple-pattern, i22150, i22900, named-patterns, named-tuple-ops,
named-tuple-ordering, named-tuples, named-tuples-xxl). dotty: `typedTuple` (`T/Typer.scala:3762-3790`):
over `MaxTupleArity` the elements become `TupleXXL.fromIArray(IArray(…))` (`:3779-3782`) typed
against `TupleXXL` and ascribed the `*:` chain (`TypeOps.nestedPairs`); the lowering's
`TupleOptimizations` and `scala.runtime.TupleXXL`.
Named tuples are `NamedTuple.NamedTuple[Names, Values]` with `withNames`/named-pattern typing
(`T/Typer.scala` `typedNamedTuple`, `Applications.typedNamedArgs` for patterns) and the
match-type operators of `NamedTuple` — out (research 21).

Port (~120 lines, XXL only): `dispatch.rs` arm `Tree::Tuple` for `> 22` elements → the typed
`TupleXXL.fromIArray` tree with the `*:` chain as type (`small_tuples.rs:40,57` already walks
`*:` chains for arities); `type_trees.rs` arm for a `Tree::Tuple` type → `nested_pairs`; tuple
patterns of that size through `Tuple.unapply`/`TupleXXL` (dotty desugars the pattern to a `*:`
chain match, `D/Desugar.scala` `tuple`); erasure of `*:` chains over 22 is `Product`/`TupleXXL`
(`S/erasure`, dotty `TypeErasure.erasedTupleArity`). Tests: i8314, i5260, tuple-erased,
tuple-patterns, tuple-patmat-size, tuple-patmat (6 by reading); tuple-concat/-cons/-map/-split/
-tail/-zip and i15988a/b need `Tuple.*` match-type reduction over long chains (#120 items 2, 4)
[I]. Named tuples (12) stay out.

### 2.8 Structural types and reflective calls (6 + 2 lowering + 1)

`{ val a: Int }`, `{ def test(a: String, b: Int): Unit }` (i4496a/b, i4528, i9404, structural,
structural-compat) are refined type trees without a parent, refused at `Y/refined_types.rs:19`
[V]; selectable-new (`class C extends reflect.Selectable`) is a block type (research 22 B) and
i18612-b/-d reach the lowering, `L/codegen/callee.rs:34-38` refuses `Selectable.selectDynamic`/
`applyDynamic` [V]. dotty: the parent is `Object` (`D/Desugar.scala` `refinedTypeToClass`),
`typedSelect` → `trySelectable` (`T/Typer.scala:939-957`) → `handleStructural`
(`T/Dynamic.scala:184-284`): `(qual: Selectable).selectDynamic("a").asInstanceOf[Int]`,
`applyDynamic("test", classOf[String], classOf[Int])(a, b)` through `structuralCall` (`:188`),
with `reflectiveSelectable` found as a conversion to `Selectable`. Lowering: nscplugin emits
`Op.Dynmethod` for `scala.reflect.Selectable.reflectiveSelectable(x).applyDynamic(…)`
(`NirGenExpr.genApplyDynamic` [I: no scala-native checkout in this worktree]) and Scala Native's
linker adds proxy methods (`Sig.Proxy`, our demangler knows them, `crates/nir/src/demangle/parts.rs:6`);
`crates/nir` has `Op::Dynmethod` (`nir.rs:194`) but the VM has no dynmethod dispatch [V grep].

Port (~400 lines): typer ~150 (`refined_types.rs:19` with `Object` as parent; `handleStructural`
in `Y/select.rs` after `extension_visible` — the `select.rs` hooks are also #42's, one coder);
lowering ~100 (`callee.rs` → `Op::Dynmethod` with the proxy signature, boxing of arguments as
`genApplyDynamic` does); VM ~150 (dynmethod lookup by proxy signature in the class's method table,
generated per class from its public methods — the linker's `GenerateReflectiveProxies`). Tests:
i4496a/b, i4528, i9404, structural, structural-compat, i18612-b/-d, selectable-new (9); i17761 and
structural-contextual (user `Selectable`, research 20 §2.8) with #42.

### 2.9 Small groups

- **By-name function types** (7): `(=> A) => Int` is refused at `Y/function_types.rs:138` [V]
  (function-arity, i14623, i20095, i5976, t7584, t7584b, t7899). dotty keeps `Function1[=> A, Int]`
  as an applied type with an `ExprType` argument until erasure (`X/ElimByName.scala:44-50`: only
  parameter types are transformed, embedded ones erase to `Function0`). Port (~80): the typer
  admits `Type::ByName` arguments of function classes (`Y/closures.rs` lambda parameters of by-name
  type as `ExprType` symbols), the lowering's `by_name.rs` treats a by-name function parameter as a
  `Function0` (closure for the argument, `.apply()` for the reference) and erasure maps the
  `ByName` argument to `Function0` (`S/erasure`). 7 tests [I].
- **Annotated types** (6): only `@repeated` passes `Y/type_trees.rs:94` [V]; `@uncheckedVariance`
  (CollectionTests, LazyLists, Signals), `@unchecked` in patterns (i16108, runtimeChecked,
  unchecked-patterns). dotty `typedAnnotated` (`T/Typer.scala:3668`) keeps any annotation as an
  `AnnotatedType`; `VarianceChecker.scala:51,135` skips `uncheckedVariance`. Port (~50): any
  annotation whose class is found → `Type::Annotated` (erasure already strips it,
  `S/erasure/mod.rs:80`; `variances.rs:127` must skip the check under `uncheckedVariance`; the
  pattern warning of §2.3 honors `@unchecked`). 6 tests [I].
- **Default arguments of later clauses** (6): `foo(1, 2, 3)()` with `(arg: String = "foo")` in
  the second clause (i4073b/c, i2939, t3488), `this(x.toString)()` (i14164) and `new
  mutable.ArrayStack[Int]` (t4535, a library default) stop at `Y/default_args.rs:33-34`, which
  refuses a default after a clause with non-copyable arguments [V]. dotty `findDefaultGetter`
  (`T/Applications.scala:352-376`): `idx = n + numArgs(fn)` and the getter is applied to the
  *previous* clauses' arguments (`defaultArgument` `:492`, `tryDefault` `:850`) — the earlier
  clause is lifted to `val`s when impure. Port (~60): lift the earlier arguments with `Y/lift.rs`
  and apply the getter to the lifted references. 5 tests [I; t4535 to probe].
- **Auto-tupling and named varargs** (6): `Some(y.size, y.name)` (unapply, virtpatmat_unapply),
  `f(3, 4)` for `f(t: (Int, Int))` (t8610), `f0(t0)` (tupled-function-apply) fall to
  `Y/named_args.rs:41` [V]; dotty tuples the arguments when the method is unary and
  `autoTuplingEnabled` (`T/Applications.scala:1676-1683`, `:290-300` for extractor arguments)
  with the migration warning. `foo(ff = bb*)` (i3207) and `f(a, b)` of `=> Int*` (byname-varargs)
  stay. Port (~40): 4 tests [I].
- **Sequence arguments of arrays** (5): `Set(a*)` with `a: Array[Int]`, `use(arr*)`, `foo(xs*)`,
  `Array[Double](pi):_*`, `new Suites(aNull*)` at `Y/repeated.rs:110-117` [V]. dotty
  (`T/Typer.scala:1320-1352`): the expected type of `e*` is `Seq[T] | Array[? <: T]`, and the
  `Typed` gets `Array[T]*`/`Seq[T]*` (`translateToRepeated(fromCls)`); `ElimRepeated` wraps an
  array passed to a Scala varargs method (`X/ElimRepeated.scala:168-169` `arrayToSeq` =
  `wrapArray`, `:194`; `L/phases/repeated.rs` has the Seq side). Port (~50): 4 tests
  (adding-growing-set, i6858, i8977, spreads; i24204 is `Null`) [I].
- **Polymorphic function types** (5): `[T] => List[T] => Option[T]` as a *type* (boehm-berarducci,
  contextbounds-for-poly-functions, i13950, i6891, polymorphic-functions) is `PolyFunction {
  def apply[T](x: List[T]): Option[T] }` (`D/Desugar.scala:1358` `makePolyFunctionType`), typed
  by `typedRefinedTypeTree`; values are anonymous `PolyFunction` classes already typed [I: no
  "PolyFunction" value refusal in the table]. Port (~120 with the erasure of `PolyFunction`
  refinements, `C/TypeErasure.scala:791,1093` `PolyFunctionOf`). 5 tests [I].
- **Derived types without a source** (4): better-fors-map-elim, better-fors-map-inlined,
  enum-List1, enum-Tree at `Y/derived.rs:21` — the enum-case parent `List[Nothing]`/`Tree[Nothing]`
  dotty derives from the variance of the parameters (`D/DesugarEnums.scala:66`
  `interpolatedEnumParent`) and the `for` strawman desugaring. Port (~60):
  `crates/syntax/src/desugar/enums.rs` passes the enum's type parameters as `watched`. 2–4 [I].
- **Union widening** (4): `List(apple, orange)` (i14705), `getOrElse(Map())` (i14582), `List(…)`
  of mixed cases (equality, numbereq): `S/subtype/join.rs:9` returns `None` where the base classes
  carry different type arguments (dotty `orDominator`'s `approximateOr` merges with `|` by
  variance). Port (~40) in `join.rs`. 4 tests [I].
- **Source versions** (8): `import language.`3.7`` (3), `language.future` (4),
  `language.`3.3`` (1) are refused as a whole (`Y/mod.rs:231`) [V]. The eight tests are about
  given prioritization (`T/Implicits.scala:1349`, `:1828-1830`: the search mode `New` from 3.6,
  `CompareErr`/`CompareWarn` before, around `compareAlternatives` `:1370`),
  `drop-apply-optimization` and modularity (`i21213*`, experimental, out). A
  `SourceVersion` field in `Ctx` that only selects the ranking rule (research 18 A, `ranking.rs`)
  frees given-disambiguation, given-triangle, implicit-specifity, implied-priority (~40 lines)
  [I]; `future` and modularity stay out.
- **Application of a non-method** (7): `B[String]()` constructor proxy with type arguments
  (creator-applys, `Y/apply.rs:64`: `Type::Poly` on a class reference), `Foo[Int](x)` extractor
  (unapply-tparam), `Hi.parse[Hello.type]` (i20395, a `given` with a type parameter applied to
  nothing), `x(0)(y)` (extmethods2), `_(0)` on a view (t4660), named tuples (namedtuples-buildcolumn,
  out), typeclass-derivation2a (#43). Singles; the proxy one is ~20 lines in
  `Y/constructor_proxies.rs` [I].
- **Member-not-found singles** (9, out): JDK and runtime API the image lacks or we do not map
  (`Runtime.getRuntime.totalMemory` t2849/t7336, `scala.runtime.Statics` t13033, `IntRef.elem`
  t8601c), `"asdf".copy(b = 3)` (t3346g), `bip.bar.duh(33L)` (t1987), `defn.Lifecycle.FromZIO.type`
  (i23279), `case Abs(4)` (caseclasses), `xxx.str` through an implicit class (convertible) — each
  to probe on its own; none shares a cause [V for the sites, I for "no shared cause"].
- **Group G leftovers** (9): type lambdas as given arguments or in `given` types
  (extension-specificity2, i13146poly, implicit-functors/2, tuple-ops) and HK lambda parameters
  (hk-alias-unification, i13332a, i24826, named-tuples-strawman-2): research 19 G2 (~400 lines),
  unchanged.

### 2.10 Lowering refusals (8) and the crash

One test each [V]: i25943 `classes in local classes capturing values` (`L/phases/nested_reads.rs:54`),
i8931 `classes in anonymous classes using the enclosing instance` (`outer_fields.rs:95`, a
trait-parameter test freed by research 22 T whose anonymous class holds a class — `ExplicitOuter`
of an anonymous class's inner class), implicitFuns `overridable or overloaded methods with
context function results` (`context_results.rs:41`), llift `members of local traits other than
methods capturing values` (`local_traits.rs:43`), t8611b `library classes nested in classes`
(`outer_args.rs:32`), t8933c `forwarders of objects extending JDK classes` (`L/codegen/statics.rs:77`),
i18612-b/-d reflective calls (§2.8). Research 15's `ExplicitOuter`/`LambdaLift` items cover the
first four (~60 lines each); t8611b needs the outer pointer of a library inner class from NIR
(`$outer` field of the library class, `L/phases/outer_fields.rs`), ~40. The crash i15618
(`ObjectArray cannot be cast to IntArray`): `def toArray: Array[ScalaType[T]]` with
`ScalaType[Int32]` a match type overriding `Array[Int]` — the bridge or the call site erases the
match type to `Object[]` where dotty reduces it to `Int` (`TypeErasure` through
`translucentSuperType`, research 21 §3) — a #120 item (erasure of match types in method
signatures, `S/erasure/mod.rs:83`), to ticket [I].

## 3. MRs

Each gated by `just gates`, `just typer-compare` on the probes that become golden inputs,
`just dotty-lower-report` and `just snapshot-diff programs lower typer`; the lowering halves add
a `tests/programs` entry under `just nir-cmp`. Sizes are Rust lines of diff [I]; tests are first
refusals from §2 that no other item blocks.

| MR | scope | lines | tests |
|---|---|---:|---|
| D dependent | §2.4 unconditional `safeSubstParams`, later clauses, eta | ~120 | HLists, HLists-nonvariant, t5656, t6443-by-name, t6443-varargs, given-eta, i11583, i4600b, i9530 (+tuples1 #120, i12919 M1, byname-implicits-6) |
| M1 mirrors | §2.1 `Y/implicits/mirrors.rs`, `MirroredElemTypes` lookup | ~450 | 17 `summon[Mirror…]` tests, i23897, i7049, poly-kinded-derives, deriving-constructor-order |
| M2 derives | §2.1 `Deriving.scala` givens | ~150 | derived-specificity, deriving-interesting-prefixes, i11961, i15101, i9011, typeclass-derivation3 |
| R retained inline | §2.2 `bodyRetainer`, erasure rename | ~200 | i15725, i17584a, quote-MacroOverride, transparent-foreach, typelevel-overrides, inline-override, i13334, i14150, erased-machine-state (+7 mirror givens after M1) |
| A inline accessors | §2.2 `inline$` accessors | ~180 | i12160, i15374, i16252, i22498, i4754, transparentPrivates, transparentProtected, transparentByName, transparentForeach (+i13215 #121, i11706 probe) |
| P type patterns | §2.3 `tryWithTypeTest`, unchecked warning, erased tests, type-variable binds | ~250 | i1099, i11914, i11914a, rescue, rescue-boundary, i20225, tryPatternMatch, patmatch-classtag, fully-abstract-nat/-1…7/-interface, type-test-nat, tuples-empty, i1991, for-desugar-strawman, i13433, i13433b (+enum-approx, i14693, i5077, tuple-accessors) |
| O overloads | §2.5 `PolyProto` resolution, receiver views, `ValueOf` | ~150 | optimizer-array-load, t0677-old, t2005, t6827, i3930, overloading-specifity, i15913, sip23-valueof, i8530-b, bigDecimalTest, i16464, impconvtimes |
| Q opaque aliases | §2.6 any alias, `elim_opaque` erasure | ~80 | i14587.opaques, i9928, ConfManagement, Signals1, Signals2, loops-alt (+i13461-b #82, string-context-… views) |
| X tuple XXL | §2.7 `TupleXXL` expressions, types, patterns | ~120 | i8314, i5260, tuple-erased, tuple-patterns, tuple-patmat-size, tuple-patmat (+6 after #120) |
| S structural | §2.8 `handleStructural`, `Op::Dynmethod`, VM proxies | ~400 | i4496a, i4496b, i4528, i9404, structural, structural-compat, i18612-b, i18612-d, selectable-new |
| K small groups | §2.9 by-name function types, annotated types, later-clause defaults, auto-tupling, array sequence arguments, poly function types, derived enum parents, union join, source-version ranking, proxy type arguments | ~600 | ~40 (7+6+5+4+4+5+3+4+4+1 at best) |

Order: D → P → O → M1 → R → A → M2 → Q → X → K (in pieces) → S, by run tests per line; about
2,700 lines for ~140 of the 480 refused tests. Milestones: after D + P + O + K the typer refusals
fall to about 395 and the top reason is mirrors; after M1 + M2 + R + A to about 350, the top
reasons then being macros (`mac"$x"`, `typeChecks`, 17) and named tuples (12), both out of scope
for now; Q, X and S take it to about 330 and S adds the first VM feature since the frontend
started (dynmethod dispatch).

Conflicts, for coders in parallel [V for the file lists, I for the overlap]:

- `Y/apply.rs`: D (result substitution, `:130-134`), O (`choose` is called from `apply.rs` and
  `select.rs`), K's defaults (`default_args.rs` from `apply.rs`) and #42 (`typed_apply` hook for
  `applyDynamic`) — D first, then O, K's defaults after; #42 is in work now, O and D rebase on it.
- `Y/select.rs`: S (`handleStructural` hook after `extension_visible`) and #42 (`tryDynamic`
  hook) — one coder, #42 first; O's receiver views touch `apply.rs`'s failure path only.
- `Y/implicits/`: M1 (`synthesis.rs`, new `mirrors.rs`), P (a `TypeTest`/`ClassTag` search from
  `patterns.rs`, `class_tags.rs` extended by `synthesizedTypeTest`), O's `ValueOf`, K's source
  ranking (`ranking.rs`) and #123 (candidate interpolation in `search.rs`) — M1 and P add files
  and can run in parallel; #123 edits `search.rs`, which none of them does.
- `Yn/checks.rs` and `Y/defs.rs`: R alone; A adds `inline_accessors.rs` and edits
  `typed_template_stats` in `Y/classes.rs` (M2's `derived$` givens also enter members there —
  A before M2).
- `Y/patterns.rs`, `Y/unapply.rs`: P alone; K's auto-tupling edits `named_args.rs`, called from
  `unapply.rs` for extractor arguments — K's tupling after P.
- `Y/type_trees.rs`: X (`Tree::Tuple` arm), K's annotated types (`:94`) and poly function types
  (the catch-all `:108`) — small arms, sequential merges, no shared logic.
- Lowering: P (`type_tests/runtime_tests.rs`), S (`codegen/callee.rs`, a new `codegen/dynmethod.rs`),
  Q (`elim_opaque.rs`), K's by-name (`by_name.rs`, `S/erasure`) and array varargs
  (`phases/repeated.rs`), R (`codegen/body.rs`, `callee.rs`) — disjoint files; `S/erasure/mod.rs` is touched
  by K's by-name and X (`*:` over 22) and by #120's match-type erasure (i15618) — one coder for
  erasure.
- #120 (match types) overlaps X (long `*:` chains), P (i13433's `Matcher[A]`), D (tuples1) and
  the i15618 crash; #121 overlaps A (i13215); #122 overlaps D (skolem prefixes in typed trees).

Risks: D substitutes skolems into every dependent application — `just typer-stress` and
`typer-compare` on the pos corpus before merging, as research 22 warned; P changes what a type
pattern *means* for every abstract type (an erased test where we refused), so the unchecked
warning must match dotty's text and position or `typer-compare` fails on warnings; R keeps
bodies of methods the lowering used to drop — `just nir-cmp` on an `inline override` program
checks the renamed body and the `Inline` flag; M1's anonymous mirrors give the lowering classes
with a `fromProduct` it must synthesize from the attachment, not from the companion; S's VM
dispatch is new runtime behaviour (ADR) and should land last and alone.
