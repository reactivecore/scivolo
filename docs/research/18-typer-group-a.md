# 18 — Typer group A: implicits

Date: 2026-10-14. Claims are [V] verified by reading the code or running it, [I] inferred. Paths:
`T/` = `ref/scala3/compiler/src/dotty/tools/dotc/typer/` (3.9.0), `C/` = `.../dotc/core/`,
`Y/` = `crates/typer/src/typer/`, `Yi/` = `crates/typer/src/typer/implicits/`, `L/` =
`crates/lower/src/`, `S/` = `crates/tasty/src/`. Measured on main 2b2cf68f against
`just dotty-lower-report` (683 pass, 709 typer refusals) [V]. `a01`–`a49` name reductions of the
tests, probed with `just typer-dump` and, for `a05`, `a08`, `a11`, `a12`, `a16`, `a23`, with
`just typer-compare` against dotty's typed trees (the oracle). Follows
[research 14](14-typer-groups.md) §A and [16](16-typer-group-f.md).

**Recommendation.** 118 of the 709 typer refusals are implicit-related by their first reason
(§1). Reading each test at its offset and reducing it gives nine causes, two of which are not in
the implicit search at all: a *frozen* bound check in the constraint refuses every `$conforms`
under an open result variable (`flatten`, `unzip`, `toMap`: 13 tests, a 10-line fix, §2.2), and
`conversions.rs:49-61` gives up on a whole view search as soon as any `given` def or `Function1`
value is among the candidates (17 tests, §2.1). Ranking (§2.3, 21 tests with CanEqual's
first-wins rule), the search history for by-name implicits and divergence (§2.4, 7), `using`
clauses in applications (§2.5, 5), synthesis of `ClassTag`/`NotGiven`/`ValueOf` (§2.6, 17),
context functions (§2.7, 3), inner-class companions and extension members of implicit values
(§2.8, 3) follow. Mirrors (14, #43), tuples (about 11, #39) and the smaller items of §2.9 stay
out. Six MRs, each under ~400 lines of Rust, in the order of §3; the first two unblock about 35
tests for under 400 lines.

## 1. The refusals

First refusals in the report whose reason or construct is implicit-related [V]; the offsets come
from `just typer-dump` over the 118 files:

| first | reason | `Y/` site | § |
|---:|---|---|---|
| 45 | no implicit argument | `Yi/search.rs:77` | 2.2 (13, plus range and i8396), 2.4 (5), 2.6 (19), 2.7 (3), 2.9 (implicit-alias, i17115), i11542a |
| 18 | ambiguous implicit arguments | `Yi/search.rs:71` | 2.3 (17), 2.6 (i3396) |
| 15 | class tag of a type that is not a class | `Yi/synthesis.rs:47-49` | 2.6 |
| 14 | given conversion | `Yi/conversions.rs:56` | 2.1 |
| 6 | implicit search too costly | `Yi/search.rs:140` | F0 (research 16 §1.3) |
| 5 | arguments not matching the clause's using | `Y/migrations.rs:43` | 2.5 |
| 4 | implicit candidate outside the subset | `Yi/search.rs:118` | 2.3, 2.9 |
| 2 | implicit function value | `Yi/conversions.rs:59` | 2.1 |
| 2 | overload resolution with implicit conversions | `Y/alternatives.rs` | B (i11938, tuples) |
| 1 each | too deep; ambiguous conversions; type variables (×2); adaptation; using after receiver; default getter | `search.rs:48,69`; `conversions.rs:66`; `adapt_by_result.rs:50`; `extension_calls.rs:55`; `default_args.rs:42` | 2.4, 2.1, 2.3, 2.8, 2.5 |

`i11542a` (listed as "no implicit argument") types today [V]. The "adaptation" refusals that are
conversions (research 14 counted them under A) were not re-dumped; `test-implicits` is one.

## 2. Causes

### 2.1 Views: `given` defs and function values abort the search (17 tests)

`Yi/conversions.rs:49-61` walks every group *before* `fitting` and returns `Err("given
conversion")` for any `given` method and `Err("implicit function value")` for any value whose
class derives from `Function1` or `Conversion` (`is_function_value`, `:74-84`) [V]. Any given def in
scope or in the implicit scope of the receiver/expected type kills the view search: `Tuple`'s
companion holds `given canEqualTuple[...](using …)`, so every member selection on a tuple-typed
value refuses (`a13` `1 *: Tuple()`; genericTupleMembers, i13968, i14215, toList, tuple-ops,
tuple-product, tuples-empty, tuples1a) [V]; a `Factory` expected type reaches `Tuple2`'s bases
(`a28` `.to(MutMap)`, t3984) [V]; a user `given () => Int` is a def (`a21`, byname-given) [V]; an
`implicit val deck: List[Card]` (a `PartialFunction`, so a `Function1`) blocks `1.isInDeck`
(`a06`, puzzler54) [V]; `implicit object f1 extends (Int => String)` blocks `5: String` (`a23`,
t8280) [V]; a `given Conversion[Int, Box]` is itself refused (`a05`) [V]. collections (`s.clone +=
(3, 4000, 10000)`), t5045 (`"…" r ("year", …)`) and i3539 (`.asInstanceOf` without type arguments,
a view search on an unbounded variable) are the same guard by their offsets [I].

dotty (`T/Implicits.scala:131-250` `candidateKind`): a method with one explicit parameter is a
conversion (`:138-143`); a value, or a nullary/implicit method's result, is a conversion only if
it derives from `Conversion` or `<:<` (`:181-190`; `Function1` values only under
`-source 3.0-migration`, `:181-184`); `$conforms` and `<:<.refl` never produce a view
(`:1258-1261`); a `FunctionN` expected type makes every value a candidate (`:231`). Applying a
conversion value: `typedImplicit` `:1201-1218` adapts the reference to `Conversion[from, pt]` and
applies `.apply` — the oracle shows `Apply(Select(Ident(given_Conversion_Int_Box), apply), [1])` with
a synthetic span at the argument and a `Typed` for an ascription (`a05`) [V]; for t8280's `ob2` it
picks `f2(5L)` with the literal narrowed (`a23`) [V]. Underspecified searches are refused up front
(`:1685-1696` `isUnderspecified`: an unbounded variable, `Any`, `AnyRef`, `Unit` results).

Port: replace `:49-61` and `is_function_value` by a candidate-kind test (`Y/implicit_views.rs:49-53`
already encodes the `Conversion`/`<:<` rule for `may_view`, keep the two in one place); in
`try_conversion` (`:110-167`) a value or nullary/implicit-method candidate whose result produces a
`Conversion` becomes `ref.apply(arg)` after its implicit arguments; other values and plain givens
are skipped, not refused; `isUnderspecified` on the goal. ~90 lines; `conversions.rs` (168) splits
into the search and a `conversion_kinds.rs`. Lowering: `Conversion.apply` is an interface call,
nothing new [I]. Tests: t8280, puzzler54, byname-given, t3984, collections, t5045, i3539, the eight
tuple tests (which then meet #39's `*:` [I]), test-implicits and instances after §2.8.

### 2.2 `$conforms` under an open result variable (13 tests, not in the search)

`List(List(1)).flatten` (`a02`), `Seq((1, "a")).unzip` (`a03`), `List(if … List(…) else Nil).flatten`
(`a24`, numbereq) and `def f[B](implicit ev: Int => B)` (`a41`) all fail with "no implicit
argument"; `summon[Int => Int]` and `summon[List[Int] => Iterable[Int]]` find `Predef.$conforms`
(`a31`), and `def f[B >: Int](implicit ev: Int => B)` passes (`a43`) [V]. Cause [V]: the candidate
`$conforms[?A]: ?A => ?A` must satisfy `Function1[?A, ?A] <: Function1[Int, ?B]`, i.e. `?A >: Int`
then `?A <: ?B`; `add_upper` (`crates/typer/src/constraint/mod.rs:126-150`) checks the new upper
bound against the lower one with `satisfiable` (`:77-87`), which runs the subtyper *frozen*, so
`Int <: ?B` with `?B >: Nothing` is false and the bound is rejected; with `B >: Int` it holds.
dotty's `addOneBound` (`C/ConstraintHandling.scala:297`) runs `isSub(lo, bound)` with the
constraint open, so `?B` gets `Int` as lower bound. Fix: `satisfiable` for a bound that mentions
another uninstantiated variable must constrain (or `add_upper`/`add_lower` must propagate to the
variables in the bound as `propagated` does for a bare variable, `:111-115`, `:140-144`). ~10-30
lines; gated by `just typer-report`, `typer-stress` and `snapshot-diff` since every inference goes
through it. Tests [I, by their offsets: `flatten`, `transpose`, `unzip`, `toMap` with `<:<`, which
has the same shape]: collections-toSelf, colltest1, flat-flat-flat, mapValues, numbereq, t2255,
t4027, t4582, t5053, t5201, t7215, t8428, transpose.

Two more one-liners found on the way, same MR: `(1L to 10L).sum` is refused while `List(1L).sum`
and `(1 to 10).sum` pass (`a34`, `a44`) [V]: `Inclusive[T]` is declared `extends NumericRange(start,
end, step, true)`, pickled as `New` of the eta-expanded `[T] =>> NumericRange[T]` with the
argument on the `TypeApply`, and `S/symbols/classes.rs` `parent_type` drops the `TypeApply`, so
`Inclusive[Long]` is not a `NumericRange[Long]` [V]; test `range`. `summon[Show[Prefix.Alias]]` with
`given C: Show[Alias]()` in `object Prefix` is refused for `type Alias = String` (`a45`, which dotty
refuses too: `collectParts` dealiases first) and for `type T >: String <: String` (`a36`, i8396),
but found for an abstract `type Abstract` (`a46`) [V]: `S/types/widen.rs` `alias_of` takes equal
bounds for an alias, so `collect_parts` dealiases `T` and never reaches `Prefix`; in 3.9
`TypeBounds.apply` (`C/Types.scala`) keeps them `RealTypeBounds`, an anchor [V].

### 2.3 Ranking (15 tests) and CanEqual (6)

`Yi/ranking.rs:11-21` returns "ambiguous" whenever a successful candidate is a `given` or has a
contravariant class in its type (`:25-43`), and `compare_candidates` (`:46-60`) ranks by owner
derivation and `as_specific` on the type without implicit clauses (`candidate_shape`, `:82`) [V].
dotty's `compare` (`T/Applications.scala:2159-2392`), used by `compareAlternatives`
(`T/Implicits.scala:1370-1401`, nesting level first `:1373`):

1. **Contravariant parts** (contrarivant `a07`, ordered `a18`, cochis, cochis-example [V]): for two
   old-style implicits the Intermediate scheme flips contravariant arguments to `arg => Unit`
   (`:2276-2284`) and compares with `relaxed_<:<`, so `String => String` beats `Object => String`
   and `Ord[Int]` beats `Ord[Any]`; `isAsGood` `:2187-2223` (a value beats a method, `:2217-2218`;
   a poly alternative is instantiated to abstract types `:2200-2212`, the other side to type
   variables `:2219-2220`); `viewExists` (`T/Implicits.scala:886-898`, a real search) makes
   `implicit def _2: Int` beat `_1: Long` for `implicitly[AnyVal]` through `Predef.int2long`
   (implicits-numeric `a17`) [V]. Our `Y/views.rs:14-42` `view_may_exist` is an approximation
   ("false only when none is"); for ranking use `converted` under a rollback instead [I].
2. **Givens** (given-triangle `a08` [V]): the New scheme prefers the general one, `tp2 <: tp1 ||
   viewExists(tp2, tp1)` (`:2286`), after `prepare` widens a given object to its parents
   (`:2266-2270`) and `widenGiven` (`:2308-2316`) the result of a synthetic given class; mixed
   implicit/given: the implicit loses (`:2252`, `:2275`). The oracle passes `given_A` for `A`
   (`a08`) [V]. Also #81 item 1's `canEqualOption`/`canEqualOptions`.
3. **Implicit parameter lists as tie-breaker** (implicit-specifity-2 `a15`, implied-specifity-2
   [V]): `:2380-2391` compares the types without implicit clauses first and, on a draw, with them
   (`Foo[Bar[T]](implicit Medium)` beats `(implicit Low)` through `isApplicableMethodRef`,
   `:2189-2199`); `compareOwner` (`:2106-2122`) steps from a module to its companion class with the
   `implicitMembers.forall(owner == larger)` guard; `comparePrefixes` (`:2341-2348`) for the same
   symbol. Ours never looks at the implicit clauses.
4. **Nesting levels and shadowing** (i18183, `.given`, `.mixed`; `a09` [V]): `ContextualImplicits.level`
   (`T/Implicits.scala:317-327`: same owner and (import or same scope) keeps the outer level) and
   `combineEligibles` (`:340-358`): at equal level an inner candidate hides outer ones *of the same
   name*, and a named import beats a wildcard one regardless of order. `Yi/candidates.rs:71-75`
   merges levels by `def_scope` without name shadowing, so the local `implicit val foo` and the
   imported `bar.foo` tie. ~25 lines in `contextual_implicits`.
5. **Open type variables** (t298 [V], tagless [V site], HLists, HLists-nonvariant [I]):
   `Yi/search.rs:68-70` refuses two fitting candidates when `pt` has type variables (`foo[List[(Int,
   Int)]]` → nested `List[?T]` with `?T <: Int`: `anyList[T]` and `intList` fit; dotty ranks the
   successes under their own typer states, `:1427-1442`). Since the winner is re-run at `:73`, the
   guard can go once ranking decides; `Y/adapt_by_result.rs:49-51` ("implicit method result under a
   type with type variables") is the same family. HLists' "candidate outside the subset" needs a
   probe build (research 14 §1) to name the nested reason.
6. **CanEqual first-wins** (numeric-range, stream-stack-overflow-filter-map, t3242b, t3529, t3923,
   viewtest; `a10` [V]): `isCoherent` (`:1281`) makes `rank` take the first success without
   disambiguation (`:1514-1515`), so `canEqualSeq`/`canEqualSeqs` never conflict; our `has_eq`
   (`Yi/can_equal.rs:71-84`) runs the reflexive search through `most_specific` and gets "ambiguous".
   ~10 lines in `search.rs:62-72`; the tree never shows the evidence, so order is not an oracle
   concern [V].

Port: `ranking.rs` (107) grows to `ranking.rs` (compare, owners, prefixes), `as_good.rs` (schemes,
flip, viewExists) and the level/shadowing change in `candidates.rs`; ~350 lines. Risks: every
multi-candidate search in the oracle corpus may change its pick (`just typer-report`,
`typer-stress` for `tests/neg` ambiguity errors).

### 2.4 Search history: divergence and by-name implicits (7 tests)

`summon[E]` with `given f(using e: E): E(e)` beside an outer `given e: E(null)` recurses to
`MAX_DEPTH` (`a19`, implied-divergence) [V]; `implicit def foo(implicit rec: => Foo): Foo` (`a11`,
byname-implicits-4) and the by-name parameter of byname-implicits-5 (`foo(implicit y0: => Int)`,
an `implicit lazy val`) fail with "no implicit argument" [V]. dotty keeps an `OpenSearch` chain
(`T/Implicits.scala:1966-2007`) per nested search: `checkDivergence` (`:1857-1908`, `typeSize` and
`coveringSet` from `C/Types.scala:2089-2094`) fails a candidate already open on a type no smaller,
unless a by-name argument lies between (then the knot is tied); `recursiveRef` (`:1927-1949`) returns
the dictionary reference, `linkBynameImplicit`/`defineBynameImplicit` (`:2036-2079`) record it, and
`emitDictionary` (`:2089-2178`) wraps the result in a block. The oracle's shape (`a11`)
[V]: `Block([TypeDef $_lazy_implicit_$2 (FINAL SYNTHETIC, parents Object, Serializable, a
SYNTHETIC constructor) { val $_lazy_implicit_$1: Foo = Foo.foo(this.$_lazy_implicit_$1) }, val
$_lazy_implicit_$3 = new $_lazy_implicit_$2], $_lazy_implicit_$3.$_lazy_implicit_$1)` — plain
`val`s whose self-reference sits inside the by-name argument; names from `NameKinds.scala:324`
(`$_lazy_implicit_$`, one fresh counter: entry, class, instance), `Symbols.scala:867-868`.
`nestedSearches` (`:2012-2019`, `:1295-1309`) is dotty's budget, the counterpart of our
`MAX_TRIES`.

Port: a new `Yi/history.rs` (the open stack of (candidate, pt, by-name) on `Typer`, divergence
check, dictionary) with `try_implicit` (`search.rs:131-175`) pushing/popping; `implicit_arg` strips
`ByName` from `pt` and `adapt.rs:31-54` passes by-name implicit arguments as it passes explicit
ones [I]. ~300 lines. Lowering: a synthetic local class with vals and a by-name closure over
`this`; local classes lower since #34, verify with `just lower-run` on `a11` and a
`tests/programs` entry under `just nir-cmp` for the `$_lazy_implicit_$N` names [I]. Tests:
byname-implicits-4, -5, -28, -30 (after F0's budget), lazy-implicit-lists, lazy-implicit-nums,
implied-divergence.

### 2.5 Applications and `using` clauses (5 tests)

- **Empty arguments and `new C` to a using clause** (i2567 `a16`, curried-using-threading `a22`,
  typelevel-numeric at `new MathLib[N] {` with `[N: Numeric]`, i9530 `f(e)` with `(using s:
  Scope)(x: s.Expr)` [V]): `Y/migrations.rs:41-44` refuses `contextual && !using`. dotty adapts the
  function to the `FunProto` first: `adaptNoArgs` (`T/Typer.scala:4941-4946`) →
  `adaptNoArgsImplicitMethod` (`:4544-4620`) inserts the implicit arguments, then the next clause
  takes the explicit ones; `realApply` `T/Applications.scala:1300-1316`. The oracle for `new Foo`
  and `new Foo()` is `Apply(Apply(Select(New(Foo), <init>), [tc]), [])` with the inner apply typed
  `()Foo` (`a16`), i.e. a using-only constructor carries a trailing `()` clause
  [V]; check that `crates/syntax` desugars it so. ~40 lines in `Y/apply.rs:53-60`.
- **Partial `using` arguments** (extra-implicits `a39` [V]): `f(using a)` for `(using a: A, b: B)`
  ends in "default getter" (`Y/default_args.rs:42`); dotty's `tryDefault`
  (`T/Applications.scala:850-896`) searches the missing implicit (`:891-893`) when there is no
  default (`:880` `canSupplyImplicits`; an empty `()` to an old-style implicit clause is the 3.7
  migration error `messages.scala:1641-1647`). ~20 lines.
- Out: i11583 (using clauses before the receiver of a right-associative extension, 1 test),
  tupled-function-untupled (`Y/extension_calls.rs:53-56`, needs `TupledFunction`, #39).

### 2.6 Synthesis (17 tests)

- **`ClassTag`** (15: Tuple-fromArray, array-existential-bound, arrays, i1284, i13131, i14964,
  i14964b, i22345, i23901, t0017, t1192, t2788, t3540, t3763, t4813 [V offsets]; `a12`, `a27`, `a29`
  [V]): `Yi/synthesis.rs:40-49` accepts a class without type parameters only. dotty
  (`T/Synthesizer.scala:32-93`): `Array[T]` → `ClassTag[T].wrap` (`:74-76`); an `AndOrType` with equal
  sides → one side (`:34-40`); otherwise `hasStableErasure` and
  `ClassTag.apply[T](classOf[erasure(T.normalizedTupleType)])` (`:77-89`). The oracle:
  `ClassTag.apply[Int](classOf[Int]).wrap` for `Array[Int]`, `classOf[Option]` for `Option[Int]`,
  `classOf[Tuple2]` for `(String, Int)` (`a12`): the class literal's constant is the erased class
  [V]. Port ~70 lines (new `Yi/class_tags.rs`), erasure from `S/erasure/mod.rs:65` (`erase`,
  `erased_class` `bounds.rs:138`); the lowering's `gen_class_literal` (`L/codegen/literal.rs:33-45`)
  erases through `gen_type`, so a `ClassLiteral` of `Option[Int]` already yields `Option` [I;
  `just nir-cmp` on a `tests/programs/classtags`].
- **`NotGiven`** (i3396 `a30` [V]): `NotGiven[Tagged[Int]]` finds `amb1`/`amb2` and reports
  "ambiguous" up through `fooNotTagged`; dotty negates each candidate's outcome (`isNotGiven`
  `:1293`, `negateIfNot` `:1534-1543`), so the nested search fails cleanly and `fooDefault` wins.
  With a nested *ambiguity* dotty also fails just the candidate unless a better one exists
  (`healAmbiguous` `:1475-1492`, `:1498-1507`); ours propagates it (`search.rs:122-123`). ~25 lines.
- **`ValueOf`** (sip23-valueof): `T/Synthesizer.scala:236-255` (`new ValueOf(literal)`), ~25 lines.
- **`TypeTest`/`Typeable`** (typable): `:95-126`, group F5. **`Manifest`** (manifest-summoning):
  out. **`Mirror`** (14: deriving, deriving-from-product-typed, i10997, i11050, i12328a, i12328b,
  i12919a, i12919b, i13332shapeless-b, i14127, i15190, i15399, scala_enum_testing, unroll-multiple):
  #43. **`TupledFunction`** (tupled-function-andThen, -compose; `:128`): #39.

### 2.7 Context functions (3 tests + `type tree Function`)

`boundary: if x < 0 then break(0)` (`a01`, break-opt, breaks) and `returning { … throwReturn(true) }`
(nonlocal-return) need the body typed against `Label[T] ?=> T`: dotty's `typedUnadapted` wraps a
non-closure term under a context function type in `makeContextualFunction`
(`T/Typer.scala:3945-3960`, `:3987-4030`), whose `given` parameters the nested `break(using
Label[T])` finds; `typedImplicit` types a reference against such a `pt` through `typed`
(`T/Implicits.scala:1193-1195`). Ours has no context function types at all (`val g: Int ?=> Int`
is "type tree Function", `a37`; the 13-test row of the report) [V]. Port: `typedFunctionType`
(`T/Typer.scala:1778`) for `?=>`, `makeContextualFunction` in `Y/dispatch.rs`/`Y/closures.rs`,
~150 lines; lowering: `ContextFunctionN` erases to `FunctionN` (names known in
`L/phases/beta_reduce.rs:74`, `eta_reduce.rs:98`; the closure phase must accept the SAM), ~60 lines
[I]. Tests: break-opt, breaks, nonlocal-return, implicitFuns, implicitFuns2 [I].

### 2.8 Inner-class companions and extension members of implicit values (3 tests)

- `val s: String = i` with `i: c.Inner` and `object Inner { implicit def … }` (test-implicits,
  `a04`) [V]: `Yi/scope.rs:100-109` returns no companion for a class nested in a class. dotty's
  `collectCompanions` (`T/Implicits.scala:706-772`) adds `TermRef(pre, companion)` (`:724-726`,
  `:736-740`) and the objects on the prefix path (`addPath` `:755-768`). `candidate_ref` already
  builds a reference through a path (`Yi/candidates.rs:152-156`). ~50 lines.
- `implicit object O { extension (x: Int) def em }` then `1.em` (instances, instances-anonymous;
  `a20`) [V]: "member not found" — dotty's `Candidate.Extension` (`:191-193`, `hasExtMethod` `:80-84`,
  companions tried `extensionOnly` `:266-267`) applies `extMethodApply(Select(ref, em), qual)`
  (`:1222-1246`, `T/Applications.scala:2982-3002`; ambiguity when a candidate is both). Ours tries
  methods only in `converted(Goal::Member)`. ~60 lines reusing `Y/extension_calls.rs:42-51`.

### 2.9 Smaller and out of scope

implicit-alias (`given t: [X] => TC = new TC`, `a26`: a polymorphic given alias is not found [V], 1
test); i17115 (`Tuple.Tail` match type, #39); extension-methods ("candidate outside the subset" at
`.max`: completing a sibling `given readerMonad: [Ctx] => Monad[[X] =>> Ctx => X]` of the same
object records a type-lambda refusal, group G [I]; `a48` passes); transparentArrowAssoc (the user's
`implicit class ArrowAssoc` with an `inline def ->` in a value class; the reduced `a25` without it
passes [V], cause [I]); i11938, tuples (overloads with conversions, `Y/alternatives.rs`, group B);
HLists ×2 (§2.3.5); the six "too costly" tests are F0's (research 16 §1.3; byname-implicits-30 also
needs §2.4).

## 3. MRs

Each gated by `just gates`, `just typer-checks`/`typer-compare` on the repros that become golden
inputs, `just dotty-lower-report` and `just snapshot-diff programs lower typer` [I]. Sizes are
Rust lines of diff [I]; tests are first refusals from §2.

| MR | scope | lines | tests |
|---|---|---:|---|
| A1 open bounds | §2.2 `satisfiable`/`add_upper`, `NumericRange` bound, alias through `This` | ~60 | collections-toSelf, colltest1, flat-flat-flat, mapValues, numbereq, t2255, t4027, t4582, t5053, t5201, t7215, t8428, transpose, range, i8396 |
| A0 views and small rules | §2.1 candidate kinds; §2.3.6 CanEqual first-wins; §2.6 NotGiven and nested ambiguity; §2.5 using clauses | ~320 | t8280, puzzler54, byname-given, t3984, collections, t5045, i3539, numeric-range, stream-stack-overflow-filter-map, t3242b, t3529, t3923, viewtest, i3396, i2567, curried-using-threading, typelevel-numeric, i9530, extra-implicits (+8 tuple tests to #39) |
| A4 synthesis and scope | §2.6 ClassTag, ValueOf; §2.8 inner companions, extension members | ~300 | the 15 ClassTag tests, sip23-valueof, test-implicits, instances, instances-anonymous |
| A2 ranking | §2.3.1-5 | ~350 | contrarivant, ordered, cochis, cochis-example, implicits-numeric, given-triangle, implicit-specifity-2, implied-specifity-2, i18183, i18183.given, i18183.mixed, t298, tagless; #81 item 1 |
| A3 search history | §2.4 divergence, by-name, dictionary; lowering check | ~340 | byname-implicits-4, -5, -28, -30, lazy-implicit-lists, lazy-implicit-nums, implied-divergence |
| A5 context functions | §2.7 typer and lowering | ~210 | break-opt, breaks, nonlocal-return, implicitFuns, implicitFuns2 |

Order: A1 → A0 → A4 → A2 → A3 → A5, by tests per line; about 1,600 lines and ~75 of the 118
tests, the rest #39 (11), #43 (14), G, B and the six of §2.9 [I]. A1 is a constraint change and
goes alone with the oracle sweep; A0 and A3 both edit `try_implicit`.

Interactions with `83-typer-f0` [V]: its only change under `Yi/` moves the `MAX_TRIES`
decrement in `search.rs:139-142` after the result-type precheck (`:149-152`), which A0's extra
candidates (givens and `Conversion` values now tried as views) and A3's history rely on — rebase
A0/A3 on it or land after it; f0's zonk fix for "inaccessible member" (research 16 §1.2) is what
`a40` (cochis reduced) hits today, and its case-lambda arity is needed before `collections` runs.
Risks: §2.2 and §2.3 change which candidate or instantiation wins silently (oracle parity through
`typer-report`/`typer-stress` before merging); §2.1 enlarges every view search (budget, #48).
