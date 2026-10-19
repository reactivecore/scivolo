# 27 — Typer and lowering groups after research 23

Date: 2026-10-19. Claims are [V] verified by reading the code or running it, [I] inferred. Paths:
`T/` = `ref/scala3/compiler/src/dotty/tools/dotc/typer/` (3.9.0), `C/` = `.../dotc/core/`, `X/` =
`.../dotc/transform/`, `D/` = `.../dotc/ast/`, `N/` = `.../dotc/inlines/`, `Y/` =
`crates/typer/src/typer/`, `Yn/` = `crates/typer/src/namer/`, `Yi/` = `crates/typer/src/inlining/`,
`L/` = `crates/lower/src/`, `S/` = `crates/tasty/src/`, `G/` = `crates/syntax/src/desugar/`.
Measured with `just dotty-lower-report` on main f734e1e5 (`tmp/dotty/3.9.0-0.5.12/lower-results.tsv`):
**1043 pass** of 1711, 372 typer refusals (157 distinct reasons), 16 lowering refusals, 13 parser
refusals, 1 frontend crash (t8199), 1 VM crash (i15618, #120), 31 "fails in the VM", 59 multi-file,
175 skipped [V]. Follows [research 23](23-typer-groups-after-22.md); its D, P (part), O, Q, X, K, S,
M1, M2, R and A have landed. Written under a hard stop: the enum, arity and poly-function groups and
the lowering/VM sections were probed (`just typer-dump`, `just lower`); the other typer groups are
clustered from the refusal texts and the sites of the texts, marked [I] where no dump was read.

**Recommendation.** The 31 VM failures and 59 multi-file tests are not frontend work (§2.14). Of the
372 typer refusals about 60 are out of scope now (macros and `compiletime` 20, named tuples 13,
#120 match types 10, #127 type tests 10, XML 3), 11 pass already (§2.2), and the long tail is
157 reasons, 110 of them singles. What remains splits into three cheap fixes (§2.2 type
variables in applied type patterns, 10 tests for ~30 lines; §2.1 enum-case parents, 12 tests
for ~60; §2.9's lambda owners, 6 for ~10), two mid-size ports (§2.7 polymorphic function types,
8 tests; §2.8 functions over 22 parameters, 8), the rest of the higher-kinded group (§2.9, ~15
tests, ~200 lines), the inline leftovers of #82 (§2.2, ~15), the overload/application leftovers
(§2.3, ~25) and three clusters that need a probe pass before coding (§2.4 adaptation 24, §2.5
member not found 19, §2.6 implicits 12). Eleven MRs, ~2,500 lines, about 150 of the 372 refused
tests (§3).

## 1. The refusals

Top refusal texts with the site that produces them [V sites by `just grep`; counts from the TSV].

| run | reason | site | § |
|---:|---|---|---|
| 24 | adaptation | `Y/adapt.rs` | 2.4 |
| 19 | member not found | `Y/select.rs`; 9 are i8577a–i (one cause) | 2.5 |
| 12 | no implicit argument | `Y/implicits/search.rs` | 2.6 |
| 11 | named tuple | `Y/dispatch.rs` — out (research 21) | — |
| 9 | application of a non-method | `Y/apply.rs:64`, `Y/apply_insertion.rs:27` | 2.3 |
| 8 | scala.compiletime operation | `Y/inline_refs.rs:35-41` — `typeChecks`, out | — |
| 8 | reference to `E[T:scala.Nothing]` (8 enum tests) | `Y/idents.rs:126` | 2.1 |
| 8 | type tree PolyFunction (6) + PolyFunction (2) | `Y/type_trees.rs:108`, `Y/dispatch.rs` | 2.7 |
| 8 | function (type) of that many parameters (7) + `Function26` (1) | `Y/closures.rs:28`, `Y/function_types.rs:134`, `Y/arg_shapes.rs:149` | 2.8 |
| 6 | named or default arguments of a repeated method, … | `Y/named_args.rs:41` | 2.10 |
| 6 | type lambda other than the right-hand side of a type definition | `Y/type_lambdas.rs` | 2.9 |
| 6 | type pattern needing a type test or type binding | `Y/patterns.rs:34` — #127 | — |
| 6 | wrong number of type arguments | `Y/type_args.rs:21`, `Y/type_trees.rs:25` | 2.13 |
| 5 | source versions other than 3.7 to 3.9 | `Y/mod.rs` — `3.3`, `future`, out | — |
| 5+4+2+3+1+1 | overloaded reference; ambiguous overload resolved by further arguments; overload resolution with implicit conversions; no applicable alternative; ambiguous overload; … against a function type | `Y/overloads.rs`, `Y/alternatives.rs` | 2.3 |
| 4 | parents giving a base class different type arguments | `Yn/base_types.rs:37` | 2.13 |
| 4 | derived type without a source | `Y/derived.rs:21` | 2.1 |
| 4 | type tree of another kind in a library inline body | `Yi/` | 2.2 |
| 4 | pattern-bound type variable | `Y/unapply.rs` — #127 | — |
| 4 | unresolved name h (inline `case h *: t`) | `Yi/` | 2.2 |
| 4 | higher-kinded type lambda parameter | `Y/type_lambdas.rs` | 2.9 |
| 4 | inline body with a local type, class or import | `Yi/` | 2.2 |
| 4 | block type with local symbols | `Y/stats.rs` — #122 / §2.12 | 2.12 |
| 3 each | cyclic reference (`Yn/complete.rs:17`); assignment to a non-variable; match type that may reduce (#120); sequence argument (`Y/repeated.rs`); higher-kinded refinement; enum class with its own ordinal; extractor result depending on its unstable argument; GADT (#41); inline method of a nested object; conflicting inherited members (`Y/overrides.rs:167`); `$scope` (XML, out); import bound; function parameter without a type; higher-kinded opaque type; method value with a by-name parameter; double definition (`Yn/checks.rs:70`, `Y/overrides.rs:89`); `@static` value; ambiguous implicit conversions; type lambda parameter with a variance | | 2.9–2.13 |

Lowering (16, §2.14): `L/phases/context_results.rs:41` (3), `L/codegen/expr.rs:114` (2),
`L/codegen/statics.rs:77` (2), `L/codegen/expr.rs:83` (2), and one each at `L/phases/type_tests/
runtime_tests.rs:58`, `nested_reads.rs:54`, `getters.rs:55`, `L/codegen/callee.rs:29`,
`outer_fields.rs:95`, `local_traits.rs:43`, `outer_args.rs:32` [V].

## 2. Causes

### 2.1 Enum-case parents and generic enum cases (8 + 4 + 8 tests)

`G/enums.rs:119-141` (`interpolated_enum_parent`) builds the parent of a simple case of a generic
enum as the *text* `List[T:scala.Nothing]` handed to `sym_ref` [V]; the typer's `typed_full_ref`
(`Y/idents.rs:124-127`) resolves a full name only and refuses "reference to List[T:scala.Nothing]"
[V]: enum-List2, enum-List2a, enum-Option, enum-Option1, i9011, typeclass-derivation1,
typeclass-derivation3, deriving-constructor-order (8). dotty's `interpolatedEnumParent`
(`D/DesugarEnums.scala:66-83`) is a `TypeTree(enumClass.typeRef.appliedTo(targs))` with the
bounds' `lo`/`hi`; ~10 lines. All eight then need the next item [I]. The four "derived type
without a source" tests (enum-List1, enum-Tree, better-fors-map-elim, better-fors-map-inlined)
are a case class taking over the enum's type parameters: `G/enum_cases.rs:74` builds
`DerivedTypeTree { FromParam, watched: None }`, `Y/derived.rs:21` refuses [V dumps]; dotty
`Desugar.derivedEnumParams`/`derivedTypeParamWithVariance` watch the enum's parameter — set
`watched`, and `derived_from_param` takes the source from the enum's primary constructor, ~25.
A generic enum case class's `apply` widening (enum-HList; `Y/enum_cases.rs:33-34` widens with
the class-level parents instead of the applied type's — dotty `widenEnumCase` uses
`tpe.parents`; ~10) [V]. Enum classes referenced by textual full name (`G/enums.rs:144-155`
`enum_class_ref`/`raw_enum_ref`): a local enum (enums-thunk "Outer._$E") or one in a braced
package (i15234, i19178 — `G/class_syms.rs:45` `child_full_name` lacks the package) — emit a
`DerivedTypeTree { TypeRef, watched: enum }` resolved by `derived.rs:31,141`, ~10 [V]. Case
companions counted as members (enums-precise, `Y/enum_case_refs.rs:61-62,93` compares names
without `is_type`; ~5) [V]. The case import of enum-Color (`G/enum_cases.rs:34-35` →
`Y/find_ref.rs:59-68` "constructor proxy shadowing a term"; resolve the companion symbol
directly, ~10) [V]. Enums extending `java.lang.Enum` (enum-constrs, enum-custom-toString,
enum-values-order: `Y/classes.rs:72-98` `defines_enum_ordinal` counts `java.lang.Enum`'s final
`ordinal()`; dotty `X/SyntheticMembers.scala:485-494` skips `ordinal` for Java enums; ~15, the
Java constructor protocol maybe ~40 more) [V]. enum-nat is §2.2's type-pattern variable.

Port (~100 lines): 12 tests for the first two items, then 8 more.

### 2.2 Type variables in applied type patterns (10 tests) and inline leftovers (#82, ~15)

**Not inline.** `case _: (h *: t)`, `case _: S[n1]`, `case bs: *:[bh, bt]` fail before any
expansion: a lowercase type identifier inside an applied type pattern is never bound —
`Y/pattern_bounds.rs:34` binds only `_` wildcards (`TypeBoundsTree { alias: None }`), so `h`
reaches `Y/idents.rs:60` as "unresolved name" [V dumps]; tuple-typetests is a plain non-inline
`def nonEmpty(x: Any) = x match { case _: (h *: t) => … }`. Tests: erased-inline-product-1/2/3,
tuple-typetests, i11542, i12328, i11706, typeclass-derivation2b, typelevel-peano, i15302a (10).
dotty: `typedAppliedTypeTree` (`T/Typer.scala:2777`: `if untpd.isVarPattern(arg) then
desugar.patternVar(arg)`) and `typedBind` on a type (`:2982`, `newPatternBoundSymbol`). Port
(~30 lines): in `Y/type_trees.rs:75`/`pattern_bounds.rs`, under the pattern-type mode a
var-pattern `Ident` argument becomes a `Bind` of a fresh `CASE` type symbol bounded by the
parameter's bounds, entered in the case scope. Adjacent to #127's "pattern-bound type variable"
(`Y/unapply.rs:193`, extractor arguments) — the same symbol kind, a different site; one coder.

**Stale rows [V].** Eleven listed tests type on main now: i13334, breaks, loops, runtimeChecked,
runtimeChecked-2, genericValueClass, i2895a, i15374, inline-override-num, i6858, i11542a (the
TSV predates the last merges). i13691b, i13961a, transparentArrowAssoc now stop at "adaptation",
i17115 at "no implicit argument", saferExceptions with a `throws` error.

**Inline leftovers [V sites].** A local anonymous class in an inline *argument* (`assert(new E1
{})`: i24357, implicitFuns2, t5665 — `Yi/copy.rs:77` refuses `TypeDef`/`Template`; dotty
`TreeTypeMap.transformDefs` re-owns the class and its members; ~100 lines, or ~20 binding the
argument to a proxy `val`); an inline method of a non-static nested object (i11914, i11914a,
i15317 — `Yi/this_proxy.rs:34`; dotty `Inliner.computeThisBindings` binds the module's `this`
to the call's prefix path; ~40); an inline method of an object with opaque types (i13461-b,
opaque-inline2 — `Y/inline_refs.rs:35` `within_opaques`; `Inliner.addOpaqueProxies`, ~100);
inline accessors of a constant-typed private (i4754, `Yi/accessor_defs.rs:48`, widen the
result, ~10) and of a polymorphic/implicit private method (transparentPrivates, `:37`, ~40);
`transparent inline given` and `inline given … ()` with a class body (i13332shapeless,
erased-machine-state — `Yn/inline_checks.rs:42,44`, lift the ban, ~15 each); a transparent
inline `unapply` (i8530 — `Yi/transparent.rs:67`, `Y/unapply.rs:49`; dotty `Inlines.inlineCall`
inside `typedUnApply`, ~40; i8530-b then leaves `L/codegen/callee.rs:29`); `inline val MV =
Int.MaxValue` (i2266, `Y/defs.rs:32`: the library's final val is not a `Type::Constant` for us,
~20); the divergence measure outside the subset (i11563, `Y/implicits/measures.rs:7`: fold
`Lambda`/`Match`/`Param` as dotty's `typeSize`/`coveringSet` do, ~25); exports of inline members
(i10724, i14020, `Y/exports.rs:138`: the forwarder is itself `inline`, ~30); erased definitions
(i16943 `Y/inline_refs.rs:47`; i13691, i23305 `Y/constructors.rs:48` `new` of an `Erased` class,
~30 + lowering); a retained `inline override` in a value class (i15725, `Yi/retained.rs:69`, #88).
i1990b (lowering "a reference to A", `L/codegen/expr.rs:83`) is an outer `self` alias in an
inline body [V read, I cause]. Port (~300 lines in two MRs): ~15 tests.

### 2.3 Overloading and application leftovers (16 + 9 tests)

All dumped [V sites]. Overloads: a tie broken by a *later* argument clause (`f(1)(2)`, `9/0
rescue 1`: extmethod-overload, i15287, rescue, rescue-boundary — `Y/overloads.rs:121` refuses
when the proto has a following clause; dotty `resolveOverloaded1` → `resolveMapped(…,
skipParamClause, pt.resultType)`, ~60 lines); applicability undecided for lambda or `m _`
arguments against type-variable or SAM formals (i11938, tuples — `overloads.rs:93-96`,
`Y/arg_shapes.rs:8` return `None`; dotty types the argument against the formal, ~40); the result
type not pruning alternatives (t1381 — `overloads.rs:137`; `constrainResult`, ~20); wildcard
arguments against type variables in invariant positions (i25000, i25000b — `overloads.rs:103`
via `is_subtype`; `TypeComparer.compareCaptured`, ~30 [I]); an overloaded reference against a
non-function `pt` (i4364a SAM `Consumer[String]`, implicit-specifity `Show[Int]` case-class
`apply` vs implicit `apply`, returning — `Y/alternatives.rs:67`; `pt.isSAMType` as `FunProto`,
`normalizedCompatible`, ~50); an overloaded `val` + `def` against a function `pt` (i19641,
`alternatives.rs:25,35`, ~20); named arguments in an overloaded application (i15913, t4592 —
`Y/dispatch.rs:79` "NamedArg"; per-alternative `reorder` + defaults, ~60); `Array(a*)` to an
overloaded method (t6611, t6611b — `Y/idents.rs:60` "unresolved name _*", `Y/apply.rs:102`; ~20);
overloaded `unapplySeq` of `Regex` (t5045, t6406-regextract — `Y/unapply.rs:40` → `alternatives.rs:88`;
`trySelectUnapply` resolves with the scrutinee as the single argument, ~30); iarrays [I].

Applications: apply insertion through a view of the function part (`x(0)(y)`, `_(0)` on a
`String`: extmethods2, t4660, typeclass-derivation2a, fully-abstract-interface —
`Y/apply_insertion.rs:35`; dotty `tryInsertApplyOrImplicit` → `tryImplicit` with a
`SelectionProto(apply)`, reuse `Y/conversions.rs`, ~30); a constructor proxy with type arguments
and a user companion without `apply` (creator-applys, `Y/apply.rs:62`, ~20); an extension method
with type parameters after the receiver called with explicit arguments (i20395,
namedtuples-buildcolumn, `apply.rs:62`; `Pt::PolyFun` through `ext_method_apply`, ~40); a
right-associative extension with a using clause before the receiver (i11583 [I],
`desugar.extMethod` swaps the clauses, ~30 in the namer); a using clause right after the receiver
(interleaving, tupled-function-untupled — `Y/extension_calls.rs:62`, ~25; untupled also needs
`synthesizedTupledFunction`, §2.6).

Port (~350 lines in two MRs): overloads ~250 (14 tests), applications ~100 (6–7 tests).

### 2.4 Adaptation (24 tests, probe first)

`Y/adapt.rs` refuses when no adaptation rule matches; the 24 tests [I by their sources]: views on
a receiver or argument that research 20 §2.7 left (extension-methods, i10511, i7960, i8073,
i8073b, mapConserve, t3048, t3199b, t3452f, tcpoly_parseridioms, string-context-implicits-with-
conversion), weak conformance / numeric widening of literals and `Seq` arguments (weak-conformance,
spreads, i16065), dependent results of research 23 D leftovers (byname-implicits-6, i4600b,
i9530, i14587.min), mirrors (deriving: `Mirror.Of` result refinement, #129), inline parameters
(inline-param-semantics, i14150, #82), `implied-priority` (given ranking), fully-abstract-nat-4
and quoted-sematics-1 (#127, macros). dotty: `adapt` (`T/Typer.scala:4360-`), `adaptNoArgs`,
`tryInsertImplicitOnQualifier` (`:4278`), `harmonize`/`weakConformance` (`T/Applications.scala`
`harmonic`, `C/ConstFold`). Port (~150 lines) after a probe pass that attributes each test to one of
the five rules; the view and weak-conformance rules are the ones with several tests (~14) [I].

### 2.5 Member not found (19 tests)

i8577a–i (9) share one source shape: a `given`/`extension` on a dependent `x.T` member reached
through an alias [I, to read]; the rest are JDK/runtime API the image lacks (t2849, t7336
`Runtime.totalMemory` — pass in Scala Native, so a VM native; t13033 `Statics`, t8601c
`IntRef.elem`), `copy` on a non-case class (t3346g), `bip.bar.duh` of a package object (t1987 is
the lowering's `expr.rs:114`, the typer counterpart is `checkValue`, `T/Typer.scala:757-768`:
`pkg` as a value becomes `pkg.package`), `case Abs(4)` (caseclasses), a view's member
(convertible), `defn.Lifecycle.FromZIO.type` (i23279), `MirroredElemTypes` on a product mirror of
a typed product (deriving-from-product-typed, #129), fully-abstract-nat (#127). Port: i8577a–i
alone after a probe (~80 lines) [I]; the JDK ones need natives.

### 2.6 Implicit search gaps (12 + 15 singles)

"No implicit argument" [I by source]: Meter, MeterCaseClass (opaque-type extension givens),
byname-implicits-17, i13433 (#127), i4073, i4600, implicit-alias, implied-specifity-2 (ranking),
manifest-summoning (`Manifest` synthesis, `T/Synthesizer.scala:700-790` `synthesizedManifest`),
tupled-function-andThen, tupled-function-compose (`TupledFunction` synthesis,
`T/Synthesizer.scala:170-230` `synthesizedTupledFunction`, with §2.8's arities), typable. Singles
by text: ambiguous implicit conversions (i3539, matchable, t8280), numeric harmonization (i9939
#60, puzzle), "implicit candidate outside the subset" (instances, instances-anonymous), implicit
import from a value (range, typelevel-numeric), search too deep (iterator-from), i23897 (#129),
tagless, typealias_overriding, byname-implicits-28, sip23-valueof (`ValueOf` without the call's
context). Port (~120 lines): `synthesizedTupledFunction` and `synthesizedManifest` beside
`Y/implicits/synthesis.rs` (3 tests), the two `instances` tests, conversions ranking (3) [I].

### 2.7 Polymorphic function types (6 + 2 tests)

`[T] => List[T] => Option[T]` as a type (boehm-berarducci, contextbounds-for-poly-functions,
i13950, i6891, polymorphic-functions, tuple-map) hits the catch-all of `Y/type_trees.rs:108`
[V site], and the two "PolyFunction" (i13304, single-case-expr) a value form the dispatcher
lacks [I]. dotty: `typedPolyFunction` (`T/Typer.scala:2092-2130`): in type mode
`desugar.makePolyFunctionType` (`D/Desugar.scala:1358-1377`: `PolyFunction { def apply[T](x:
List[T]): Option[T] }` as a `RefinedTypeTree` with the `PolyFunctionApply` attachment), in term
mode `typedPolyFunctionValue` against `defn.PolyFunctionOf(poly)` (`:2103`), with context bounds
eliminated (`elimContextBounds`). Erasure: `C/TypeErasure.scala:791,1093` `PolyFunctionOf` erases
to `PolyFunction`'s `apply(Object*)`. Ours types poly-function *values* already (no value refusal
in the table but the two above) and refined type trees (research 20 N4), so the type form is
`makePolyFunctionType` in `G/` plus the `PolyFunction` refinement's erasure in `S/erasure`.
Port (~120 lines). Tests: 6–8 [I].

### 2.8 Functions over 22 parameters (4 + 3 + 1 tests)

`function_class(n)` resolves `scala.Function{n}` by name (`Y/function_types.rs:17-24`); the image
has none above 22, so closures (`Y/closures.rs:28`: functionXXL, implicitFunctionXXL,
tupled-function-extension-method, tupled-function-tupled), function types
(`Y/function_types.rs:134`: context-functions, i2004, i2004b) and `Function26` as a name (i1915)
stop [V sites]. dotty synthesizes `FunctionN` for N > 22 (`C/Definitions.scala:221`
`newFunctionNType`, `:1624-1635` `FunctionSymbol`/`FunctionType`; `isSyntheticFunctionClass`
`:1704`) and erases them to `FunctionXXL` (`:1720-1724` `functionTypeErasure`;
`C/TypeErasure.scala:283` erases an anonymous function of more than 22 parameters to
`apply(xs: IArray[Object])`; `X/FunctionXXLForwarders.scala` adds the forwarder to classes
implementing one). Ours: `S/erasure/special.rs:44` already erases `FunctionN`/`ContextFunctionN`
above 22 to `FunctionXXL` [V]; the lowering's `L/phases/casts.rs:85` reads arities by class name.
Port (~150 lines): synthetic `FunctionN` symbols (a trait with `apply` of N parameters, entered
on demand as `ContextFunctionN` presumably is), closures of such a type lowered as
`FunctionXXL` with an `apply(IArray)` unpacking its elements (`L/codegen/closures`), and
`FunctionXXLForwarders`. Tests: 8 [I].

### 2.9 Higher-kinded group (research 19 G2, ~20 tests)

Probed [V sites, I fixes]; much cheaper than research 23 assumed. A type lambda in a position
other than a type definition's right-hand side (`Foo[[A] =>> T => A]`, `given Functor[[T] =>>
Tuple1[T]]`: extension-specificity2, i13146poly, implicit-functors, implicit-functors2,
poly-kinded-derives, tuple-ops) stops at `Y/type_lambdas.rs:17` because `type_def_with_rhs`
finds no owner for the parameters — dotty's `typedLambdaTypeTree` owns them by `ctx.owner`;
~10 lines for 6 tests. Higher-kinded lambda parameters (`type Foo[F[_], T]`: hk-alias-unification,
i24826, named-tuples-strawman-2, i13332a; `:35,59`): lambda-typed bounds in `Type::Lambda`
parameters and `type_arity`, ~30. Parameters with a variance (i24056, safeThrowsStrawman/2,
`:32`): keep the flag, ~10. Higher-kinded refinements (curried-mirror, i13332, i13332anonprod,
`Y/refined_types.rs:46`): `type_def_info` typing a `LambdaTypeTree` rhs, ~10 after the two
above. Higher-kinded opaque types (i18638, i5527, opaque-immutable-array-xm, `Y/type_defs.rs:44`;
dotty `Namer.opaqueToBounds`), ~30. Singles: `Y/type_apply.rs:38` HK method type argument
(i13087, ~15), `Y/implicits/unsettled.rs:26` `X[A]` with an HK type variable (i13986, i13987;
dotty `wildApprox`, ~25), `Y/type_lambdas.rs:96` HK alias overriding a type member (i22888,
~15), `Y/type_trees.rs:158` `new TypeTest { … }` inferring the parent's arguments from `pt`
(i4364b, type-test-binding; dotty `typedNew` → `inferTypeParams`, ~30), `Y/constructors.rs:89`
(i13332intersection, ~10), `Y/type_args.rs:36,105`, `Y/select.rs:132` (tyql, t2594_tcpoly,
t7985b, ~15 each). ~200 lines for 18–22 tests.

### 2.10 Argument lists (6 + 4 + 3 + 4 tests)

All dumped [V sites]. Auto-tupling of n arguments into one tuple parameter (t8610 `f(3, 4)`,
unapply `Some(y.size, y.name)`, virtpatmat_unapply — `Y/named_args.rs:41` via `Y/apply.rs:78`;
dotty `matchArgs` wraps `untpd.Tuple(args)` when there is one formal, ~25 lines); a named
argument for a repeated parameter (i3207, `named_args.rs:39`, ~15); by-name repeated `=> T*`
(byname-varargs, byNameVarargs — `apply.rs:74` `repeated_elem` misses `ByName(Repeated)`, then
`Y/repeated.rs:100-132`; ~15 + the lowering of by-name sequences); a default in a later clause
(t3488 `foo {…}()`, i14164 `this(x.toString)()` — `Y/default_args.rs:33-34`; dotty `tryDefault`
applies the getter to the earlier clauses' lifted arguments, `lift_app`, ~30); default getters
of `new` through an alias (t4535 `ArrayStack` = `ArrayDeque`, dealias before the companion
lookup, ~10) or with a missing empty clause before an implicit one (UnrolledBuffer, ~10
`default_args.rs:47-181`); a lifted qualifier of a curried method with defaults (i8903,
`named_args.rs:181`; `liftApp` over the whole clause chain, ~30); a result type depending on an
implicit-clause parameter (enrich-gentraversable, `Y/dependent_results.rs:65`: substitute the
inferred given's path, ~40). Experimental multi-spreads (i11419, spreads-subtyping,
spread-patterns; ~150) and `TupledFunction` apply (tupled-function-apply) stay. Port (~180 lines):
10 tests.

### 2.11 Extractors and patterns (3 + 2 + 2 + 3 tests)

All dumped [V sites]. An `unapply` with an implicit clause (t3353, t6111 — `Y/unapply.rs:64`;
`with_using_args` after the scrutinee clause, ~15 lines); generic-tuple `*:` extractor results
(i11008, i11008b — `Y/unapply_results.rs:65`, `Y/unapply_seq.rs:31`; dotty `productSelectorTypes`
tries `tupleElementTypes` first, ~20); a dependent `unapply` result `SuccOpt { type Refined <:
nat.type }` (fully-abstract-nat-5/6/7 — `unapply_results.rs:42`; keep the skolemized result, only
`get`/`isEmpty` are used, ~15); an extractor with explicit type arguments (unapply-tparam
`Foo[Int](x)` — surfaces as "application of a non-method"; dotty `trySelectUnapply` builds
`TypeApply(Select(fn, unapply), targs)` with the class parameters as fallback, ~40). Closures:
parameter untupling with declared types, a SAM `pt` or a generic-tuple formal
(automatic-tupling-…, i7788, i5257 — `Y/untupled.rs:34-48`, `Y/closures.rs:69`; dotty
`ptIsCorrectProduct`, `decomposeProtoFunction`, ~40); parameter types inferred from the target of
a wildcard (16405, `closures.rs:55`; `inferredFromTarget`, ~40); a SAM with a type member fixed by
a refinement (i18315, `closures.rs:69`; `SAMType.unapply` through `memberInfo`, ~20); a SAM class
with a repeated constructor parameter (i15855, `Y/sams.rs:135-138`, ~10). Port (~200 lines): 12
tests.

### 2.12 Structural leftovers (2 + 2 + 4 + 2 tests)

Research 23 S landed `reflectiveSelectable`; left: structural refinements with a context
parameter (structural, structural-compat — `handleStructural` with `using` clauses,
`T/Dynamic.scala:230-284`), user `Selectable`s (i17761, structural-contextual — `selectDynamic`
on a class deriving `Selectable`, research 20 §2.8, #42's `select.rs` hook), block types with
local symbols (i13703, i14340, selectable-new, tcpoly_monads — #122), assignment to a non-variable
(Signals2 `this.expr = expr`, t2754 `v.foo += 10` — `Y/control.rs:117-132` rewrites only
structural setters on identifier qualifiers [V dumps]; dotty `typedAssign`: a mutable `TermRef`,
else `Apply(Select(qual, setter), rhs)`, ~25 lines shared with `op_assign.rs`; i4528 types now;
i4496b reaches the lowering as "abstract variables", `L/phases/getters.rs:55` [V]). Also dumped:
`catch handler` expressions (i8662, liftedTry — `Y/tries.rs:27`; dotty `typedTry(ParsedTry)`,
~30), an extension overloaded with a plain method of the same name (sticky-extmethod,
`Y/extension_calls.rs:28`, ~5), extensions through conditional givens and their ranking
(fragables-extension `Y/extension_views.rs:65`, i9928 `:27`; ~70), `classOf[Test.type]`
(classof-object, `Y/type_apply.rs:47`, ~5), `@threadUnsafe` not accepted (i1856, i4451,
`Yn/annotations.rs:53`, ~1), `export` inside an extension block (Parser, `Yn/export_paths.rs:19`,
~80), `import scala.runtime.BoxedUnit` (i4205: a Java-source library class the image lacks).
Port (~250 lines): 12 tests.

### 2.13 Small groups and singles

- **Parents with different type arguments** (4, dumped [V]): `Yn/base_types.rs:34-37,96-98`
  `same_base_type` gives up on variant or higher-kinded arguments (CollectionTests `Seq[+A]`
  with `SeqLike[A, Seq]`, i10068d `Function0`'s covariant result, iterables, t2857 aliased
  arguments) — dotty's `baseType` joins by variance (`&`/`|`); ~40 lines.
- **Cyclic reference** (3 [V]): a recursive method without a result type overriding a member
  whose signature comes through parent type arguments (t3502, t7200 — `Yn/complete.rs:16-17`,
  `Yn/inherited.rs:14` finds no match; as-seen-from before comparing, ~20) and a self-referential
  match-type alias with a declared bound (Providers — `complete.rs:54-57`; dotty `typeDefSig`
  sets provisional bounds first, ~10).
- **Double definition / conflicting members** (6 [V]): synthetic case-class members clashing with
  user ones (i209 `copy`, i5823, typeclass-derivation2d `apply` — `Y/overrides.rs:88-89`,
  `Yn/checks.rs:69-70`; dotty `Namer.invalidateIfClashingSynthetic`, `T/Namer.scala:976-1014`,
  ~30); an overriding trait whose win comes through its *self type* (i12032, i8582,
  productElementName — `overrides.rs:161-175` looks at base classes only; dotty
  `extendedOverriddenSymbols`, ~10). Also `abstract override` not counted as an override (t4996,
  `overrides.rs:136`, 1 line), an abstract redeclaration of a concrete member (i7359,
  `:133-134`, ~3), `this(…)` in a secondary constructor of a polymorphic class (i3189, i8064,
  `Y/apply.rs:178-180`; dotty copies the class parameters to the constructor, ~25), inherited vs
  outer definitions (NestedClasses, `Y/qualifies.rs:44-54` uses all members where dotty uses own
  `decls`, ~5), wildcard imports of inaccessible members (triemap-hash, `Y/imports.rs:193-198`,
  ~10).
- **Wrong number of type arguments** (6): i13691b, i13961a, i15998, i17115, saferExceptions,
  transparentArrowAssoc — `throws`/`CanThrow` and kind-polymorphic `AnyKind` arguments; singles.
- **Source versions** (5, `Y/mod.rs:276` [V]): four are `import language.future` (given-
  disambiguation, i10930, i21213, i21213-min) — accept `future` as 3.9 semantics, ~5 lines;
  `3.3` (drop-apply-optimization) stays refused.
- **Probed singles** [V sites]: `@static val` with a non-literal rhs (i5924b, i5924c, statics,
  `Y/check_static.rs:44`; lowering ~30), import bounds `import A.{given B}` (i18183.given,
  i18183.mixed, implied-for, `Y/import_tree.rs:63`; ~40), method values of by-name parameters
  (i20095, i5976, t7899, `Y/eta.rs:58`; ~20 + lowering), opaque types in classes or at top level
  (i12914, option-extract, i6716, `Yn/checks.rs:50,52`; ~30), dependent function types with
  named parameters (eff-dependent, ift-return, `Y/type_trees.rs:122` "type tree ValDef"; dotty
  `typedDependent`, ~80; t1939 `Y/closures.rs:121`, ~20), context closure parameters of an open
  type (break-opt, loops-alt, `Y/closures.rs:59`; ~15), deferred givens (abstract-givens,
  `Yn/checks.rs:60`; ~10), `throws` desugaring and erased `CanThrow` adaptation (saferExceptions,
  i13691b, i13961a; `T/Typer.scala:3734`, ~40), generic number literals (genericNumLits,
  `Y/literals.rs:43`; ~60), skolems in inferred definition types (i23179, distinct,
  `Y/infer.rs:177,182`; ~30), `new` of a parameterized alias and alias lambdas in patterns (i4557,
  t13418; ~15 each), i23409 (~10).
- **Unprobed** [I]: handler expressions (2), secondary constructors of polymorphic classes (2),
  GADT (3, #41), `classof-object`, `pkgobjvals`, i4205, i1856/i4451 annotations.

### 2.14 Lowering refusals (16), the crash and the VM

Lowering [V sites, I causes unless noted]: "overridable or overloaded methods with context
function results" (ConfManagement, config, implicitFuns; `L/phases/context_results.rs:38-43`
flattens only methods that are alone — dotty `X/ContextFunctionResults.scala:22`
`annotateContextResults` flattens any method and `integrateContextResults` `:70` adapts the
callers; overriding pairs flatten consistently because both carry the annotation; ~60 lines);
"a selection of a value that is not a field" (t1987, t5604: `bip.bar.duh` on a package object,
`L/codegen/expr.rs:114` — the receiver is the package; `gen_module` of the package object, ~20
lines); "forwarders of objects extending JDK classes" (t8933c, tryPatternMatch, `statics.rs:77`;
~40); "a reference to xs/A" (LazyLists: an extension receiver inside a nested `def` after
`LambdaLift`; i1990b: outer `self` in an inline body, §2.2; `expr.rs:83`); array-erasure
(`runtime_tests.rs:58`, generic arrays of arrays, ~30); i4496b abstract `var` (`getters.rs:55`,
~30 with the setter); i8530-b (§2.2); research 15's four: i25943, i8931, llift, t8611b (~60
each). The frontend crash t8199 is `writing NIR: File name too long` at
`crates/lower/src/bin/lower.rs:35` — a 200-character class name as file name [V]; hash long
names (~10 lines). The VM crash i15618 is #120. The 31 "fails in the VM" are 30 "fails in Scala
Native too" (reflection Scala Native lacks: `getConstructors`, `getEnumConstants`, …; `main`
returning a value) and i19394 ("compile failed" in Scala Native's plugin) — platform, ADR 0008,
`docs/conformance.md:126-130` [V]; no frontend work. The 59 multi-file tests need the report to
compile a directory in `_N` groups through our frontend (research 26 lists it for macros).

## 3. MRs

Each gated by `just gates`, `just typer-compare` on the probes that become golden inputs,
`just dotty-lower-report` and `just snapshot-diff programs lower typer`; lowering halves add a
`tests/programs` entry under `just nir-cmp`. Sizes are Rust lines of diff [I]; tests are first
refusals from §2 that no other item blocks.

| MR | scope | lines | tests |
|---|---|---:|---|
| E enum parents | §2.1 applied-type parent, `watched` for derived parameters, generic case `apply`, enum refs, Java enums | ~160 | enum-List1, enum-List2, enum-List2a, enum-Option, enum-Option1, enum-Tree, i9011, typeclass-derivation1, typeclass-derivation3, deriving-constructor-order, better-fors-map-elim, better-fors-map-inlined (+ up to 8 singles) |
| F functions XXL | §2.8 synthetic `FunctionN`, `FunctionXXL` closures and forwarders | ~150 | functionXXL, implicitFunctionXXL, tupled-function-extension-method, tupled-function-tupled, context-functions, i2004, i2004b, i1915 |
| Y poly function types | §2.7 `makePolyFunctionType`, `PolyFunction` erasure, value forms | ~120 | boehm-berarducci, contextbounds-for-poly-functions, i13950, i6891, polymorphic-functions, tuple-map, i13304, single-case-expr |
| B type-pattern variables | §2.2 var-pattern arguments of applied type patterns as `Bind`s | ~30 | erased-inline-product-1/2/3, tuple-typetests, i11542, i12328, i11706, typeclass-derivation2b, typelevel-peano, i15302a |
| H higher-kinded | §2.9 lambda owners, HK lambda parameters, variances, HK refinements and opaques, singles | ~200 | extension-specificity2, i13146poly, implicit-functors, implicit-functors2, poly-kinded-derives, tuple-ops, hk-alias-unification, i24826, named-tuples-strawman-2, i13332a, i24056, safeThrowsStrawman, safeThrowsStrawman2, curried-mirror, i13332, i13332anonprod, i18638, i5527, opaque-immutable-array-xm (+ i13087, i13986, i13987, i22888, i4364b, type-test-binding) |
| I1–I2 inline leftovers | §2.2 classes in inline arguments, nested-object `this`, opaque proxies, accessors, inline givens, transparent `unapply` | ~300 | i24357, implicitFuns2, t5665, i11914, i11914a, i15317, i13461-b, opaque-inline2, i4754, transparentPrivates, i13332shapeless, erased-machine-state, i8530, i8530-b, i2266, i11563 |
| V1 overloads | §2.3 later-clause ties, lambda/SAM applicability, result pruning, overloaded references, named arguments, `unapplySeq` | ~250 | extmethod-overload, i15287, rescue, rescue-boundary, i11938, tuples, t1381, i4364a, implicit-specifity, returning, i19641, i15913, t4592, t6611, t6611b, t5045, t6406-regextract |
| V2 applications | §2.3 apply insertion through views, proxy/extension type arguments, using clauses after the receiver | ~100 | extmethods2, t4660, typeclass-derivation2a, fully-abstract-interface, creator-applys, i20395, interleaving |
| W arguments and extractors | §2.10–2.11 auto-tupling, later-clause and alias defaults, by-name varargs, implicit extractor clauses, `*:` results, skolemized results, untupling, SAM shapes | ~380 | t8610, unapply, virtpatmat_unapply, i3207, byname-varargs, byNameVarargs, t3488, i14164, t4535, UnrolledBuffer, i8903, enrich-gentraversable, t3353, t6111, i11008, i11008b, fully-abstract-nat-5/6/7, unapply-tparam, automatic-tupling-…, i7788, i5257, 16405, i18315, i15855 |
| J adaptation + implicits | §2.4 views/weak conformance after a probe pass, §2.6 `TupledFunction`/`Manifest` synthesis, `instances` | ~270 | ~14 adaptation tests, manifest-summoning, tupled-function-andThen, tupled-function-compose, instances, instances-anonymous |
| L lowering | §2.14 context results of overridable methods, package-object selections, JDK forwarders, abstract vars, long file names | ~200 | ConfManagement, config, implicitFuns, t1987, t5604, t8933c, tryPatternMatch, i4496b, array-erasure, t8199 |
| U structural + small | §2.12–2.13 setter selections, `catch` handlers, extension singles, `using` refinements, parents, synthetic clashes, self-type overrides, cyclic completions, `future` imports | ~400 | Signals2, t2754, i8662, liftedTry, sticky-extmethod, classof-object, i1856, i4451, structural, structural-compat, i17761, structural-contextual, given-disambiguation, i10930, i21213, i21213-min, CollectionTests, i10068d, iterables, t2857, i209, i5823, typeclass-derivation2d, i12032, i8582, productElementName, t4996, i7359, t3502, t7200, Providers, i3189, i8064, NestedClasses, triemap-hash |

Order: B → E → H → L → F → Y → V1 → V2 → W → I1–I2 → J → U, by run tests per line (B is 10
tests for 30 lines, E 12 for 60 before its singles, H's first item 6 for 10; L is cheap and
independent); about 2,900 lines for ~175 of the 372 refused tests (12 of which pass already,
§2.2, §2.12). Milestones: after B + E + H + L the typer refusals fall to about 320 and the
lowering's to 6; after F + Y + V1 + V2 + W to about 260, the top reasons then being adaptation
and member not found, both probe-first clusters; I, J and U take it to about 210, where the long
tail of singles (~90 reasons), macros, named tuples and #120/#127 remain.

Conflicts, for coders in parallel [V for the file lists, I for the overlap]:

- `G/enums.rs` and `Y/derived.rs`: E alone; `Y/idents.rs:124` (`typed_full_ref`) is E's only
  typer edit.
- `Y/function_types.rs`, `Y/closures.rs`: F; Y adds a `G/` desugaring and a `type_trees.rs` arm
  — disjoint from F's class lookup; W's auto-tupling edits `named_args.rs` only.
- `Y/overloads.rs`, `Y/alternatives.rs`, `Y/apply.rs`: V alone; W's defaults (`default_args.rs`)
  and tupling are called from `apply.rs` — V first, W rebases; #130's error reporting edits the
  failure paths of `apply.rs`/`overloads.rs` now, so V waits for it.
- `Y/unapply.rs`: W (skolemized results, extractor shapes) and #127 (type tests on the scrutinee)
  — #127 first.
- `Yi/` and `Y/inline_refs.rs`: I1–I3 alone, sequential; research 26's macro work touches
  `Yi/` for splices — one coder for the inlining crate at a time.
- `Y/implicits/`: J (`synthesis.rs`, new `tupled_functions.rs`), #123 (`search.rs`), #129
  (`mirrors.rs`) — disjoint files.
- `Y/adapt.rs`: J only; #130 reports type mismatches from the same function — J after #130.
- `Y/select.rs`: U's setter selections and user `Selectable`s share #42's hooks — one coder.
- Lowering: L (`context_results.rs`, `codegen/expr.rs`, `statics.rs`, `getters.rs`, `bin/lower.rs`),
  F (`casts.rs`, closure codegen), Y (`S/erasure` for `PolyFunction`) — disjoint; `S/erasure` is
  also #120's (match-type erasure, i15618) — one coder for erasure.

Risks: E changes every generic enum's desugaring — `just desugar-checks` and `typer-compare` on
`tests/programs` enums before merging; F introduces synthetic classes into the universe (symbols
without TASTy), which the REPL's universe rollback (#70) and the lowering's class-name arities must
tolerate — a `tests/programs` XXL entry under `nir-cmp`; I1–I3 change inline expansion for every
inline match with binders — `just typer-stress` on the pos corpus; J's view and weak-conformance
rules widen what `adapt` accepts, so `typer-compare` on warnings (dotty warns on weak conformance
of literals) must stay green; L's context-result flattening of overridable methods changes
signatures visible to overriding classes in other units — only for methods whose overrides are in
the same unit, as dotty does through the annotation, else keep the refusal.
