# 16 — Typer group F and the rest of `moderate.scala`

Date: 2026-10-14. Claims are [V] verified by reading the code or running it, [I] inferred. Paths:
`T/` = `ref/scala3/compiler/src/dotty/tools/dotc/typer/` (3.9.0), `X/` = `.../dotc/transform/`,
`D/` = `.../dotc/ast/`, `Y/` = `crates/typer/src/typer/`, `L/` = `crates/lower/src/phases/`. Measured
on main 2b2cf68f with `just dotty-lower-report` refreshed today: 683 of dotty's `tests/run` pass,
709 stop at the typer, 39 at the lowering (ROADMAP's 670/709/53 are from 2942a19) [V]. Probes
(`m1`–`m5`: `moderate.scala` with one blocker removed at a time; `p1`–`p8`: minimal repros) were
scratch files; F0's repros live on as typer golden tests. Follows [research 14](14-typer-groups.md) §F.

**Recommendation.** `moderate.scala` has five blockers, found by removing them one at a time: the
`List[?]` type pattern (group F), a member lookup on an instantiated type variable (a 5-line typer
bug), the budget of the implicit search for `Ordering[(Int, String)]` (group A, #48), `{ case (a, b)
=> … }` against a two-parameter function type (a 30-line fix in `typed_case_lambda`), and the `f`
interpolator, which is a macro and needs an intrinsic expansion or phase 4. With the first four
patched by hand (`m4`) the program types; with `s` for `f` (`m5`) it lowers and runs with the expected
output [V]. Group F itself is 86 first refusals in the refreshed report: 23 type patterns, 17
extractors "needing a type test" (mostly a guard that fires on constant-typed scrutinees), 11
refutable pattern definitions, 11 named or typed sub-patterns, 10 sequence-extractor result types,
and 14 smaller ones. Six MRs, each under ~500 lines, in the order of §3; F0, F1 and F4 are what the
exit criterion needs.

## 1. What blocks `moderate.scala`

In the order the refusals appear (`just typer-dump`, then `just lower-run`) [V]:

1. **`case xs: List[?]`** (line 25): "type pattern needing a type test or type binding",
   `Y/patterns.rs:33` (`is_checkable_type`, `:101-106`, rejects wildcard arguments). Group F, §2.1.
2. **`es.map(_.salary).sum / es.size`** (line 39): "inaccessible member" from `Y/select.rs:74`
   (`check_reference`, `Y/access.rs:19,81`). Symptom [V, `p2a`, `p4c`, `p4d`, `p7b`]: `es.sum / es.size`,
   `es.sum.toInt`, `es.sum + 1.0` and `List[Int].sum.toInt` are refused; `es.max.toInt` (`max` returns
   `A`), `es.sum[Double].toInt`, `{ es.sum }.toInt` and `g(1.0).toInt` with `def g[B >: Double](x:
   Double): B` pass [V, `p6a`–`p6e`, `p7c`]; `es.sum.toString` is typed as `double2Double(es.sum).toString`
   while `es.head.toString` selects `Any.toString` [V, `p6f`, `p8`]. Cause [I]: the qualifier of the
   selection has the type variable `B` of `sum[B >: A](implicit num: Numeric[B]): B` as its type; the
   implicit application in `adapt` (`Y/adapt.rs:31-53`) instantiates it, but `typed_select` looks the
   member up on the un-zonked `Type::Var` (`select.rs:29-36` → `Y/members.rs:24-84`, which has no `Var`
   arm: `:80` `class_symbol` → none), finds nothing, and falls back to a view (`converted`,
   `select.rs:56`) whose member fails the access check. Fix: `zonk` the qualifier type in
   `typed_select`, or a `Type::Var` arm in `members_within` (`instantiated_selected`, `:90-99`, is only
   reached through `Type::Sym` infos). ~5 lines. Nine run tests record "inaccessible member" first
   (i14432, i23477, i4754, i7375, quoted-sematics-1, t2127, t8002, tuple-patterns,
   typeclass-derivation2a); whether for the same reason is unchecked [I].
3. **`.sortBy((w, c) => (-c, w))`** (line 48): "implicit search too costly", `Y/implicits/search.rs:139`
   (`MAX_TRIES = 400`, `:17`); `summon[Ordering[(Int, String)]]` alone is refused, `sortBy(p => p._2)`
   passes [V, `p4a`, `p4b`]. Cause [I]: `Ordering.Tuple2[T1, T2](implicit ord1, ord2)` runs two nested
   searches, each over every contextual candidate (Predef's conversions and wrappers) plus the
   `Ordering` companion, and the winner of `fitting` is run a second time (`:73`
   `try_implicit(winner)`), so every nested search runs twice. Six run tests: OrderingTest,
   byname-implicits-30, range-unit, t1500b, t1500c, t4930. Fix: count only candidates that pass the
   result-type precheck (`:148-152`) against the budget, or keep the winner's tree instead of
   re-running it (the constraint must then be replayed). ~30 lines; group A (#48, research 14 A5).
4. **`staff.foldLeft(Option.empty[Employee]) { case (None, e) => … }`** (line 50): "single abstract
   method type outside the subset" (`Y/function_types.rs:50`, reached from `Y/closures.rs:28`);
   `foldLeft(0) { case (acc, e) => acc + e.age }` too, the explicit lambda with an inner match passes
   [V, `p5a`–`p5c`]. Cause [V]: `typed_case_lambda` (`Y/matches.rs:77-95`) always builds a one-parameter
   lambda `x$1 => x$1 match { … }`; dotty's `typedMatch` (`T/Typer.scala:2194-2197`) takes the arity
   from `decomposeProtoFunction(pt, 1)` and `makeCaseLambda(cases, Exhaustive, nparams)`
   (`D/Desugar.scala:2004`) builds `(x$1, x$2) => (x$1, x$2) match { … }` for a `Function2` expected
   type. Fix: the arity from `function_parts`/`sam_parts` of `pt`, a tuple selector. ~30 lines. The
   ten run tests with this first refusal (bitsets, convertible, i10068a/c, i11676, i15855, i23897,
   i7788, …) are SAMs of abstract classes, group D [I].
5. **`f"…"`** (lines 40, 45): the lowering refuses "inline call not expanded at 44:11" [V, `m4`]:
   `StringContext.f` is a macro (`inline def f(inline args: Any*): String = ${ StringContextMacro… }`),
   outside #82's inline bodies. Scala Native's NIR of `tests/programs/moderate` shows what dotty's
   macro produces (`just nir-dump tests/programs/moderate`): `StringOps$.format$extension("Total area:
   %.3f; largest: %s", ScalaRunTime.genericWrapArray(Array[Object](box(total), maxBy(...))))`, i.e.
   `augmentString(<parts joined, `%s` inserted where a `$x` has no conversion>).format(args*)` with
   the arguments boxed [V]. Options: phase 4 (macros in the VM), or an intrinsic in the typer's inline
   phase that builds that call and checks the conversions the way `StringContextMacro` does (`%d`
   against a `Double` is a compile error); the intrinsic is ~150 lines plus an ADR ("intrinsic macros":
   which library macros the frontend expands itself) and is verified byte for byte with `just nir-cmp`
   on `tests/programs/moderate` [I].

Everything else in the file (enum `ordinal`, `groupBy`, `for … do` with a tuple pattern,
`groupMapReduce`, `maxBy`, `copy` with a named argument, `(a, b)` and `Int`/`String` type patterns on
`Any`) types, lowers and runs today [V, `m5`]. Timing against the ≤ 50 ms criterion is measured once
the five land.

## 2. Group F in the refreshed report

First refusals today [V]; the constructs behind the ones dumped (`just typer-dump` over the tests,
then read at the reported offset):

| first | reason | `Y/` site | §
|---:|---|---|---|
| 23 | type pattern needing a type test or type binding | `patterns.rs:33` | 2.1 |
| 17 | extractor of a type that needs a type test | `unapply.rs:56` | 2.2 |
| 11 | refutable pattern definition | `matches.rs:109` | 2.3 |
| 11 | named or typed pattern | `unapply.rs:31` | 2.4 |
| 10 | sequence extractor result type | `unapply.rs:61-62` | 2.5 |
| 4 | pattern variable of an intersection type | `patterns.rs:79` | 2.6 |
| 3 | type arguments of a pattern of an open class | `unapply.rs:151` | 2.6 |
| 3 | pattern-bound type variable; 3 GADT; 1 extractor with more parameters | `unapply.rs:173,156,53` | out |

The "Annotated" row of research 14 (`for` generators with patterns, `@switch`) is gone: fors,
switches, t6646 and Course-2002-09 pass now [V].

### 2.1 Type patterns (23)

Of the 16 dumped [V]: **W** wildcard-applied class types, 6 (`Map[_, _]` null-and-intersect,
`boundary.Break[_]` rescue-boundary, `x2.Y[_]` t13418, `A[_]` t1434, `IAtomicValue[_]` t3097, `List[_]`
value-class-extractor-2); **S** singletons, 5 (`DFBit.type` i16806, `this.type` i23875, `Y.type`
i3200c, `bippy.type` t4577, `A.type` of a parameter t4897); **L** literal type `"a"`, 1 (i6996); **T**
abstract types, 3 (`T` of an inline method i20225, `E <: Throwable` of an extension rescue, a type
member `ExceptionType <: Exception` tryPatternMatch); an alias of a union, 1 (i24357). Not dumped:
enum-nat, erased-inline-product-1..3, i1099, i11914, i11914a [I].

dotty's typer (`T/Typer.scala:1289-1383` `typedTyped`): `cases` (`:1297-1305`) rewrites `x: T` to
`x @ (_: T)`, as `Y/patterns.rs:14-30` does; `handlePattern` (`:1361-1377`) types the type tree with
`mapPatternBounds = true` (`:4169-4185`: a wildcard argument becomes a pattern-bound type symbol
`_$1` with bounds `? & pt`, `Bind`-wrapped in the tree and dropped again by `indexPattern` in
`typedCase`), constrains it against the scrutinee (`constrainPatternType`, GADT, not needed), ascribes
with `isWildcard = true`, tries `tryWithTypeTest` (`:1389-1414`: an abstract `T` with a `TypeTest[pt,
T]` or `ClassTag[T]` in scope becomes the extractor `tt(e)`; category T) and `checkMatchable`. **No
checkability test lives in the typer**: it is `X/TypeTestsCasts.scala:388-393` `checkTypePattern` →
`whyUncheckable` (`:57-183`), an *unchecked warning* at erasure, cases: `X <:< P`; singleton; abstract
`P`; `Array[T]`; applied `P` whose arguments are not determinable (`typeArgsDeterminable`, `:75-132`;
`List[?]` on `Any` is determinable, no warning); `|`/`&`; refinements; local classes; `X` a union.
Our guard refuses W, S, L and T outright [V]. The golden tests compare diagnostics with dotty's
(`crates/typer/tests/typed.rs:40-42`), so the warning text (`UncheckedTypePattern`) matters there; the
dotty report does not see warnings.

Lowering today [V]: patmat `L/patmat/plan.rs:145-158` casts and emits `Test::Type`, `L/patmat/types.rs:41-61`
builds `$isInstanceOf[T]` plus the outer test, `L/type_tests/mod.rs:55-86` ports `transformTypeTest`
(unions and intersections through `combined.rs`, tuple types and generic arrays refused in
`runtime_tests.rs:8-16`, singletons, `Null` and `Nothing` refused at `:83`, classes in classes refused
by `explicit_outer::unsupported_test`, `L/explicit_outer.rs:79-91`, 3 tests), `is_instance_of`
(`:90-145`) ports `transformIsInstanceOf` (`X/TypeTestsCasts.scala:201-277`). W erases to the class
(`erase_boxed(List[?])` → `List`) and reaches `isInstanceOf[List]` like dotty's `:356-357` [I; byte
equality through a `tests/programs` entry and `just nir-cmp`]. S and L: dotty's pattern matcher emits
`scrutinee.isInstance(tp)` for singleton types (`X/PatternMatcher.scala:871-873`), which
`D/tpd.scala:1045-1055` makes `scrutinee.ensureConforms(Object) eq <singleton>` for reference
singletons and `<singleton> == scrutinee` for string constants and primitive singletons; ours refuses
at `plan.rs:147-149`. T: the test erases to the bound (`isInstanceOf[Throwable]` for `E <: Throwable`)
with the warning; with a `ClassTag`/`TypeTest` given, the `tt(e)` extractor (later, with group A's
synthesis of `ClassTag`).

Port: typer — drop the guard for class types with wildcard arguments, singletons and literal types
(`is_checkable_type`), `mapPatternBounds` in `typed_type` for the pattern-bound wildcard symbols as
dotty's typed trees show them (`just typer-compare` decides the printed form), `whyUncheckable`
cases 1-6 and 10 for the warning; ~200 lines in `Y/patterns.rs` and a new `Y/uncheckable.rs`.
Lowering — `types.rs::type_test` for singleton and literal targets (`eq`/`==`, ~40 lines), the
`mod.rs:83` singleton case then unreachable from patterns. Abstract `T` (3 tests) in a later MR with
`tryWithTypeTest`.

### 2.2 Extractors "needing a type test" (17)

`unapply.rs:55-58` requires the pattern's own type (`pattern_type`, `:138-181`) to be a checkable class
type. Of the 8 dumped, 6 have a constant-typed scrutinee: `""` (1938-2, `unapply(s: String): this.type`),
`"hello"` (i11008, result `String *: EmptyTuple`), `"even"` (patmat-spec), `5` (t0005, t0048: `val q =
new … X` with `unapply(v: Int)`), `1` (unapply-tparam, `unapply[T](x: T)`): `sel` conforms to the
parameter type, so `own = sel` is a `Type::Constant` and `is_checkable_class_type` is false although no
cast happens [V]. dotty (`T/Applications.scala:1943-1959`): `ownType = selType.dropAnnot(Unchecked)`
when `selType <:< unapplyArgType`, never a checkability test; the `Typed(result, ownType)` wrapper
only when `ownType.stripped ne selType.stripped` (`:1970-1971`), then `tryWithTypeTest` on it. Fix:
skip the guard when `stripped(own) == stripped(sel)` (`:75`), and treat the other case as §2.1. ~10
lines. i1991 (`unapply(foo: Foo)`, `Foo` a class type parameter with a `ClassTag`) is category T;
i11008b (`unapplySeq` returning `Int *: Seq[String] *: EmptyTuple`) is §2.5's non-empty-tuple case.
Not dumped: fully-abstract-interface, fully-abstract-nat, -nat-1..4, i14587, i8530, type-test-nat —
by their names `TypeTest` givens, T [I].

### 2.3 Refutable pattern definitions (11)

Ours (`Y/matches.rs:99-119`) ascribes `@RuntimeChecked` only when `is_irrefutable` (`:124-144`) holds
and refuses otherwise. dotty (`T/Typer.scala:2259-2290`): when the selector carries a check mode and
is not annotated `@unchecked`/`@RuntimeChecked`, `checkIrrefutable` (`T/Checking.scala:1073-1183`)
*reports* and the match is rebuilt with the `@RuntimeChecked` selector in every case (`:2283-2287`).
The report is a migration warning for pattern definitions (`ForComprehensionUncheckedPathDefs(3.2,
future)`, `config/MigrationVersion.scala:16`; message `:1081-1096` with the addendum `:1118-1130`:
"pattern's type … does not match the right hand side expression's type …" or "pattern binding uses
refutable extractor `X`", "If the narrowing is intentional, this can be communicated by adding
`.runtimeChecked` after the expression, which may result in a MatchError at runtime.") and an error for
generators without `case` since 3.4 (`:15`), which no run test relies on [V]. `checkIrrefutable`'s
recursion (`:1145-1182`): `Bind`, `UnApply` (type conformance with widening and `isTupleXXLExtract`,
`:1137-1143`, then `Space.isIrrefutable`, then the argument types from `UnapplyArgs`), `Alternative`,
`Typed`, `NamedArg`, wildcard. `X/patmat/Space.scala:318-328` `isIrrefutable`: result `Some[…]`,
`<:< true`, a synthetic Scala 2 case-class `unapply`, `unapplySeqTypeElemTp` exists (so
`List.unapplySeq`'s `UnapplySeqWrapper` counts as irrefutable: correct-bind, t3488, trailingCommas
get no warning), a product match, `isEmpty <: false`, `NonEmptyTuple`, `TupleXXL.unapplySeq`. Ours
checks the `SYNTHETIC` flag only (`:137-140`) [V].

The 9 dumped [V]: `val Array(who, what*) = …` (correct-bind), `val List(_*) = …` (t3488), `val
List(x, y, z: _*,\n) = …` (trailingCommas), `val Seq(1, 2) = LazyList(1, 2)` (t4709), `val s @ (_s:
String) = x` with `x: Object` (t8395: warns), `val B(_) = null` with `unapply[T](x: Any): Option[B[T]]`
(i5067b: warns), `val (Const, List(`x`, _, a), b) = (0, List(0, 1337, 1), 2)` (tuple-patmat-extract),
`val q"class $name extends $parent" = new Object` through an implicit `StringContext` class with
`q.unapply(arg: Any): Option[(Any, Any)]` (i1748, i1779; i1773 is the same with typed holes, §2.4).
Not dumped: case-class-23, i10178.

Port: `is_irrefutable` → `Space.isIrrefutable` plus the conformance rule, the warning text (golden
tests compare it), the `@RuntimeChecked` ascription in every case; ~80 lines (`Y/irrefutable.rs`).
Lowering: nothing — a match with the `MatchError` fall-through (`L/patmat/plan.rs:96-104`); `m5`'s
`for (d, es) <- …` already lowers and runs with its `@RuntimeChecked` selector [V].

### 2.4 Named or typed sub-patterns (11)

`unapply.rs:28-32` refuses a `Typed` or `NamedArg` argument pattern. dotty types each argument
against its component type (`UnapplyArgs.typedPatterns`, `T/Applications.scala:288-330`), refines a
tuple selector type from typed arguments first (`typedUnApply`, `:1723-1733`: `x @ (_, y: Int)` on
`(Any, Any)` gives `x: (Any, Int)`), and reorders named arguments by the case accessors
(`tryAdaptPatternArgs` `:221-229`, `productUnapplySelectors` `:248-264`, `D/Desugar.scala:1896`
`adaptPatternArgs`). Dumped [V]: typed, 7 — `case A(x: Int)` (catch-sub-cases), `HCons(x: Int, tl)`
(enum-HList), `Array(i: Long)` on `Any` (i14693), `Tuple1(a: A @unchecked)` (i16108; also needs
annotated type trees, the "annotated type" refusal of unchecked-patterns), `for (Foo(x: Int) <- xs)`
(irrefutable), `Some(x: Int)` (run-bug4840), typed interpolator holes (i1773); named, 2 — `val
C(someName = xx) = C(3)` (i22900a), `val User(name = n1) = myUser` (i24926); not dumped: t493, i23776.
Port: typed ~40 lines (drop the guard; the tuple refinement), named ~80 (`adaptPatternArgs`: names to
positions, wildcards for the rest). The lowering handles the resulting `Typed` and `Bind` patterns
already (`plan.rs:145-173`) [I].

### 2.5 Sequence extractor result types (10)

`unapply.rs:60-66` accepts only `get`-wrapped sequences (`get_match` then `seq_elem_type`,
`Y/unapply_seq.rs:20-28`). dotty's `UnapplyArgs.unapplySeq` (`T/Applications.scala:211-219`, `:268-271`)
tries, on the result and then on `get`'s type: `unapplySeqTypeElemTp` (`:118-138`: `apply(Int)`,
`lengthCompare(Int)` *or* `length`, `drop(Int)`, `toSeq`), the product-sequence match
(`isProductSeqMatch` `:78`, `productSeqSelectors` `:176`: `_1 … _(n-1)` and a last `Seq`), and the
non-empty-tuple sequence match. Dumped [V]: bare `Seq[Int]` (i6490), bare `(String, Seq[Int])` (i3248d,
i6490b), `Some[(String, Seq[Int])]` (i3248, i3248b), synthetic from `case class Foo(name: String,
children: Int*)` (i3248c) and `D(a: A, b: B[_]*)` (t6541), a custom `Data` with `length` but no
`lengthCompare` (i4984c) and one with overloaded members (i4984e, "overloaded extractor member",
`Y/unapply_results.rs:56`); not dumped: string-extractor. Lowering: `L/patmat/extractors.rs:63-83`
plans `is_sequence(result)` and `get` → sequence, refuses a product and a sequence (`:77`); dotty's
`unapplyProductSeqPlan` (`X/PatternMatcher.scala:367-378`) binds the selectors then the sequence
(`unapplySeqPlan`, `:327`). Port: typer ~70 (`unapply_seq.rs`: the three matches, `length`,
overloads by the `Int` parameter), lowering ~60 (`extractors.rs`/`sequences.rs`).

### 2.6 Smaller items

- **Pattern variables of an intersection type** (4: i1354, i1354b, matchable, t6111 — `case a: (Float |
  Boolean)` on `Int | Double`, `case x: Int` on an unbounded `T`): `typed_bind` refuses at
  `patterns.rs:77-81` where dotty assigns `body1.tpe & pt` (`T/Typer.scala:2975-2981`). ~10 lines; the
  erasure of `&` in the lowering is assumed (unions are, research 14 C) [I].
- **Type arguments of a pattern of an open class** (3: `case List(from)` on `Any`/`AnyRef`, `case head
  +: tail`): `pattern_type` refuses variant arguments of a non-final, non-case class (`unapply.rs:145-152`);
  dotty's `maximizeType` (`T/Inferencing.scala:488-513`) instantiates co- and contravariant variables
  (`fromBelow = v == -1`), only invariant ones become pattern-bound symbols (#41). ~20 lines.
- **Pattern-bound type variables** (enum-approx, i5077, tuple-accessors), **GADT** (gadts, i14983,
  i16785), **extractors with implicit parameters** (t3353): out, #41.

## 3. MRs

Each gated by `just gates`, `just typer-checks`/`typer-compare` on new golden inputs (warnings
included), `just dotty-lower-report` and `just snapshot-diff programs lower` [I].

| MR | scope | lines | tests |
|---|---|---:|---|
| F0 small fixes | §1.2 zonk, §1.3 budget, §1.4 case-lambda arity, §2.2 constant scrutinees | ~120 | OrderingTest, t1500b/c, t4930, 1938-2, i11008, patmat-spec, t0005, t0048, unapply-tparam, the "inaccessible member" nine if same cause; repros `p2a`, `p4b`, `p5a` as golden inputs |
| F1 type patterns | §2.1 W, S, L; `whyUncheckable` warning; lowering singleton tests; program `typepatterns` with `nir-cmp` | ~400 | null-and-intersect, rescue-boundary, t13418, t1434, t3097, value-class-extractor-2, i16806, i23875, i3200c, t4577, t4897, i6996, i24357 |
| F4 `f` interpolator | §1.5 intrinsic, ADR | ~200 | `tests/programs/moderate` through `just lower-run` and `scalai` strict; the ≤ 50 ms measurement |
| F2 refutable definitions and binds | §2.3, §2.6 intersections and open-class arguments | ~250 | correct-bind, i1748, i1779, i5067b, t3488, t4709, t8395, trailingCommas, tuple-patmat-extract, case-class-23, i10178, i1354, i1354b, matchable, t6111, matchnull, matchonseq, t3702 |
| F3 sub-patterns and sequences | §2.4, §2.5 (typer and lowering) | ~400 | catch-sub-cases, enum-HList, i14693, i16108, irrefutable, run-bug4840, i1773, i22900a, i24926, t493, i23776, i3248, i3248b/c/d, i4984c/e, i6490, i6490b, t6541, string-extractor, i11008b |
| F5 abstract type tests | §2.1 T with `tryWithTypeTest` (needs group A's `ClassTag` synthesis) | ~250 | i20225, rescue, tryPatternMatch, i1991, fully-abstract-*, type-test-nat |

Order: F0 → F1 → F4 (the exit criterion: `moderate.scala` through the own frontend), then F2, F3,
F5 by tests per line. About 1,600 lines in all [I]. Risks: F1 changes the printed type trees of every
pattern with a wildcard argument (oracle parity through `typer-compare` before merging); the warning
texts of F1 and F2 must match dotty's exactly for the golden tests; F4 is the first macro the frontend
expands itself and needs the ADR to say which ones follow (`s` and `raw` are plain methods [V]).
