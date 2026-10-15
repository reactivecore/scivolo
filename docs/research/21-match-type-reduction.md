# 21 — Match-type reduction for the typer (the "Tuple" and `scala.compiletime` refusals)

Date: 2026-10-15. Claims are [V] verified by reading the code or running it, [I] inferred. Paths:
`T/` = `ref/scala3/compiler/src/dotty/tools/dotc/typer/` (3.9.0), `C/` = `.../dotc/core/`, `X/` =
`.../dotc/transform/`, `Y/` = `crates/typer/src/typer/`, `Yl/` = `crates/typer/src/inlining/`,
`Yu/` = `crates/typer/src/unpickle/`, `S/` = `crates/tasty/src/`, `L/` = `crates/lower/src/`.
`tmp/dotty` (the `dotty-lower-report` data) is absent in this checkout, so the test counts are
taken from [research 20](20-member-lookup-adaptation.md) §2.3/§3 (main aed90cdc) and from a grep
of `ref/scala3/tests/run` [V for the grep, I for the counts]. `mt01`–`mt05` name reductions run
through `just typer-dump` on this branch [V]. Follows research 20 §2.3 and issue #39.

**Recommendation.** dotty reduces a match type in one place, `MatchType.reduced` →
`MatchReducer.matchCases` (§2), and consults it from subtyping, `normalized`/`translucentSuperType`
and erasure. The structure is small once the cases are kept: since 3.4 the patterns are the
SIP-56 "specced" shapes (`MatchTypeCasePattern`), matched *structurally* against the scrutinee,
no constraint solving; disjointness is a separate predicate (`provablyDisjoint`). Our TASTy
decoder drops the scrutinee and the cases (`Type::Match(bound)`), so nothing can reduce today
and six refusal reasons stand in for it (§1). Four MRs (§4, ~750 lines): keep the cases (M1),
reduce in the subtyper with `provablyDisjoint` for classes, singletons and constants (M2), use
reduction in the typer and admit match types in library inline bodies (M3), and
`constValueTuple`/`summonAll` (M4). About 20 of the ~33 tests of the "Tuple" and
`scala.compiletime` reasons follow; the rest wait on mirrors (#43) and named tuples.

## 1. The refusals

Reasons the typer gives today, by reduction [V], and the tests behind them [I]:

| reason | `Y/` site | repro | tests |
|---|---|---|---|
| adaptation: an unreduced `Tuple.Head[…]`, `Tail`, `Union`, `Zip` against the declared type | `adapt.rs` | — | tuples1b, toList, tuple-ops, i23897, i4600b, i9530 (research 20 §1) |
| match type that may reduce (a ground application of a match alias in a type tree) | `type_trees.rs:65-66` | `mt02` `type H = Tuple.Head[(Int, String)]` | tuples1, i13332a, i12328a/b, anon-mirror-gen-local (`Tuple.Elem[m.MirroredElemTypes, 1]`, #43 first) |
| type tree MatchTypeTree (a user-written match type) | `type_trees.rs:104-107` | `mt03` `type Elem[X] = X match { … }` | named-tuples-strawman-2, tyql, i7868, typelevel-peano, Providers, enum-nat [I] |
| compile-time operation type (`scala.compiletime.ops` in a type tree or a transparent result) | `type_trees.rs:29-36`, `transparent.rs:71-72` | `mt05` `val x: 1 + 2 = 3` | i7868, typelevel-peano, enum-nat, Providers (with the row above) |
| match type in a library inline body | `Yu/admitted.rs:22,29,34` | `mt01` `x.head` through `just lower-run` [V] | the tuple tests above, i17115 (`A[Tuple.Tail[Tup]]`) |
| scala.compiletime tuple operation (`constValueTuple`, `summonAll`) | `Yl/compiletime.rs:51-53` | — | constValueTuple, i15988a, summonAll, i15988b, named-tuples, typeclass-derivation* (with `Tuple.Size`) |

`x.head` on `x: (Int, String)` and `(1, 2, 3).toList` *type* in `typer-dump` (`mt01`, `mt04`): the
inferred types are the unreduced `Tuple.Head[(Int, String)]` and `List[Tuple.Union[(Int, Int,
Int)]]` [V], and the refusal comes when such a type meets a declared one (adaptation) or when the
frontend inlines the library body (`runtime.Tuples.apply(this, 0).asInstanceOf[Head[This]]`):
`just lower-run tmp/mt01.scala` stops with "match type in a library inline body" [V]. "Tuple" in the report (15 tests, ROADMAP) is the union of the
rows; the 12 `scala.compiletime` ones split between the last three rows and `erasedValue` matches
with type bindings (`Yu/cases.rs:82`, typeclass-derivation1/2a/2b/2d/3, i11174, i11563, i9473,
i13146: these need mirrors too, #43).

## 2. How dotty reduces

**Representation** [V]. `MatchType(bound, scrutinee, cases)` (`C/Types.scala:5238`); a case is a
`HKTypeLambda` over its captures whose result is `MatchCase[pattern, body]` (`caseType` `:5243-5246`,
`alternatives` `:5248`); a case without captures is the bare `MatchCase`. TASTy writes
`MATCHtype bound scrutinee case*` and `MATCHCASEtype pattern body` (our printer
`S/print/inline.rs:112-113` already shows both). `typedMatchTypeTree` (`T/Typer.scala:2860-2883`)
types bound, selector and cases (`typedTypeCase` binds the pattern's type variables as the lambda's
parameters) and, without a written bound, infers it as the `|` of the case bodies (`:2870-2881`,
3.6 migration warning); `assignType` is `MatchType(bound, sel.tpe, cases.tpes)`
(`T/TypeAssigner.scala:523-525`).

**Entry points** [V]. `tryNormalize` (`C/Types.scala:1630-1641`): strip applied aliases down to the
`MatchType` (`underlyingNormalizable`, `AppliedType` override `:4782-4790`, which also stops at an
applied `scala.compiletime.ops` type) and return `reduced.normalized`, or the constant fold for the
ops (`tryCompiletimeConstantFold` `:4808`). `normalized = tryNormalize.orElse(this)` (`:1625`);
`AppliedType.translucentSuperType = tryNormalize.orElse(superType)` (`:4758-4763`). Callers:
`isSubType` with a `MatchType` on the left (`C/TypeComparer.scala:567-571`) or right (`:822-827`)
and applied match aliases through `superTypeNormalized` (`:1459`, `:1534`); `tupleElementTypesUpTo`
(`C/TypeUtils.scala:66-84`, `dealias.normalized` at each `*:` step); the inliner's proxy types
(`inlines/Inliner.scala:619`); erasure (§3); `X/TupleOptimizations.scala:99` folds `Tuple.Size`.

**`MatchType.reduced`** (`C/Types.scala:5256-5333`) [V]: cached per match type, keyed on the
"footprint" (the type parameters, type variables and GADT-bound symbols the scrutinee and cases
mention, with their current bounds, `:5270-5307`); computes
`MatchReducer.matchCases(scrutinee.normalized, cases.map(MatchTypeCaseSpec.analyze))` under a
typer-state snapshot that is reset afterwards (`:5317-5327`), so reduction never leaks constraints.

**`MatchTypeCaseSpec.analyze`** (`:5406-5426`) [V]: a case is `SubTypeTest(pattern, body)` when it
has no captures, `SpeccedPatMat(captureCount, pattern, body)` when `tryConvertToSpecPattern`
(`:5462-5554`) recognizes a SIP-56 legal pattern, `LegacyPatMat` otherwise (an *error* under
`-source 3.4+`, `C/TypeComparer.scala:3972-3974`), `MissingCaptures` when a substitution lost a
capture. The pattern grammar (`MatchTypeCasePattern` `:5365-5380`): `Capture(n, isWildcard)`;
`TypeTest(tpe)` (no captures inside); `BaseTypeTest(classRef, argPatterns, needsConcreteScrut)`
for an applied *class* in covariant position — a `TupleN[…]` pattern is first rewritten to nested
`*:` pairs (`:5474-5475`); `CompileTimeS(arg)` for `S[n]`; `AbstractTypeConstructor(tycon, args)`
for an applied abstract type or type parameter; `TypeMemberExtractor(name, capture)` for
`P { type N = t }`. Anything else in a capture-bearing pattern is a `MatchTypeCaseError`.

**`MatchReducer.matchCases`** (`C/TypeComparer.scala:3644-4014`) [V]: `recur` over the cases in
a frozen constraint (`:4014`): `Disjoint` → next case; `Reduced(tp)` → `tp.simplified`; `Stuck`,
`NoInstance`, `ReducedAndDisjoint` (an empty scrutinee) and no case left → `NoType` (the type stays
unreduced; a warning for the no-case situation, `:4008-4009`). Per case:

- `matchSubTypeTest` (`:3710-3721`): `necessarySubType(scrut, pattern)` → reduced, else
  `provablyDisjoint` → disjoint, else stuck.
- `matchSpeccedPatMat` (`:3724-3920`): `rec(pattern, scrut, variance, widenedAbstract)` fills an
  `instances` array *structurally*: a non-wildcard `Capture` takes the scrutinee (its `hi`/`lo`
  for `TypeBounds` by variance, "not specific" when the scrutinee had to be widened from an
  abstract type, `:3738-3760`); a wildcard takes `hiBound`/`loBound`; `BaseTypeTest` takes
  `scrut.baseType(cls)` and matches the arguments by the class's variances (`:3766-3776`,
  `matchArgs` `:3885-3890`, concreteness via `MatchTypes.isConcrete`); `AbstractTypeConstructor`
  needs `scrutTycon =:= tycon` (`:3778-3783`); `CompileTimeS` needs a literal `n > 0`
  (`:3785-3790`); `TypeMemberExtractor` selects the member on the scrutinee or a skolem of it and
  drops the skolem from an alias (`:3792-3883`). On success the pattern is instantiated and the
  real test is `scrut <:< instantiatedPat` (`:3909-3911`); otherwise `provablyDisjoint(scrut,
  origPattern)` (`:3895-3897`) decides disjoint or stuck.
- `matchLegacyPatMat` (`:3922-3966`): the pre-3.4 algorithm with `caseLambda` constraints and
  `canWidenAbstract`; not needed for 3.9 sources.

**`provablyDisjoint`** (`:3063-3263`) [V]: both sides go to their *disjointness boundary* ⌈tp⌉
(`:3072-3131`: class refs and applied class types stay, `Singleton` → `Any`, enum values and
constants stay, `And`/`Or` and HK lambdas stay, everything else — abstract types, type params,
proxies — to its `superTypeNormalized`, GADT bounds first). Then (`:3133-3262`): `Nothing` is
disjoint from everything; `|` on either side needs both parts, `&` one part; two lambdas by arity;
two `TermRef`s by symbol, two constants by value, singleton vs. class by `derivesFrom`; two class
types by `provablyDisjointClasses` (`:3270`: a `final`/`sealed` decomposition, enum cases as
classes) or a common base class with disjoint *invariant* arguments (`provablyDisjointTypeArgs`
`:3321`). A `pending` set guards recursion through base types.

## 3. What we have

- `Type::Match(TypeId)` is the bound only (`S/types/mod.rs:79-80`); the decoder takes child 0 of
  `MATCHtype` and `MATCHtpt` (`S/types/decode/mod.rs:164-171`), scrutinee and cases are not
  decoded; `map.rs:39`, `show.rs:55`, `show_user/mod.rs:119` follow [V].
- Subtyping: a `Match` on the left goes to its bound (`S/subtype/fourth.rs:25-28`), on the right
  no arm in `third.rs`, so `Int <: Head[(Int, String)]` is false [V]; `atoms.rs:155-158` takes
  the bound's atoms as an upper approximation. `S/subtype/hk.rs:176-198` `nested_pairs` already
  rewrites `TupleN[…]` to `*:` chains (the inverse of `tupleElementTypes`) [V].
- Typer: `seen_from.rs` `normalized` (`:157-159`) refuses to beta-reduce an alias whose body is a
  match type, so `Tuple.Head[(Int, String)]` stays an `Applied` of the alias (`mt01`) [V];
  `type_trees.rs:65-66` refuses a ground one in a written type; `type_trees.rs:104-107` refuses
  `MatchTypeTree`, which the parser builds (`crates/syntax/src/parse/types/match_type.rs:12`,
  `parse/defs/typedef.rs:38-40`) [V]; `Yi/measures.rs:53` places a `Match` outside the subset for
  implicit ranking; `type_trees.rs:29-36` and `transparent.rs:71-72` refuse `scala.compiletime.ops`
  applications — although `Yl/type_eval.rs` already folds those ops on constants (dotty's
  `tryCompiletimeConstantFold`) for inline expansion [V]; `Yu/admitted.rs:18-41` refuses any
  library inline body whose types contain a match type or a match alias (`is_match_alias`
  `:45-55`); `Yl/compiletime.rs:51-53` refuses `constValueTuple`/`summonAll`, `Yu/cases.rs:82`
  type bindings (`case _: (t *: ts)`) in library inline matches [V].
- Erasure: `S/erasure/mod.rs:83` erases `Match(p)` to the erasure of the bound [V]; the lowering
  (`L/`) has no other `Match` use [V, grep]. dotty erases an applied match alias through
  `checkedSuperType` = `translucentSuperType` (`C/TypeErasure.scala:773-779`, `:901-902`), i.e.
  the *reduced* type, and only an unreducible one through the bound (`TypeProxy` arm `:796-797`);
  `isGenericArrayElement` takes the `|` of the alternatives (`:366-368`); `tupleArity`
  (`:95-108`) and `X/TupleOptimizations.scala:99` need `*:` chains and `Tuple.Size` normalized [V].

So once `Yu/admitted.rs` lets `val h = x.head` through, `h: Head[(Int, String)]` would erase to
`Object` where dotty erases to `Int` and unboxes: the lowering's erasure must reduce too, or the
NIR differs [I].

## 4. The port

The pieces dotty needs for the tests above, in the order of their dependencies. Sizes are Rust
lines of diff [I].

**M1 — keep the cases (tasty, ~120 lines).** `Type::Match { bound, scrutinee, cases }` with a
case as `Type::Lambda { params: captures, result: MatchCase }` and `Type::MatchCase(pattern,
body)` (dotty's `MatchCase[P, B]` applied type, a new variant is simpler than a synthetic class);
decode `MATCHtype`/`MATCHCASEtype`/`MATCHtpt` (`decode/mod.rs:164-171`, lambda captures through
`lambda_type` `:173`), `map.rs`, both `show`s, `erasure/mod.rs:83` (still the bound, until M2),
`fourth.rs:28`, `atoms.rs:155`, `Yu/admitted.rs` (unchanged, still refusing). Gate: `just
typer-stress` and the TASTy decode tests (every match type of the library decodes: `Tuple.*`,
`NamedTuple.*`, `compiletime.ops`).

**M2 — reduce (tasty subtype, ~350 lines).** New `S/subtype/match_types/`: `spec.rs`
(`analyze`: `SubTypeTest`, `Specced` with `Capture`, `TypeTest`, `BaseTypeTest` incl. the
`TupleN` → pairs rewrite via `nested_pairs`, `CompileTimeS`; `AbstractTypeConstructor` and
`TypeMemberExtractor` return `Unsupported`, which leaves the type unreduced and the typer refuses
"match type pattern outside the subset"); `reduce.rs` (`matchCases` with the four results; the
structural `rec`, `base_type` of the scrutinee for the pattern's class — the subtyper's base-type
walk in `applied.rs`/`refs.rs` [I: check it returns the applied base], the final `scrut <: pat`
with the captures substituted through `subst`); `disjoint.rs` (`provablyDisjoint` with
`disjointness_boundary`, `Nothing`, `|`/`&`, constants and `TermRef` singletons, `final` classes,
sealed decomposition from the children the universe knows, common base class with disjoint
invariant arguments; `pending` set). No type variables are needed: the specced algorithm
instantiates captures structurally and the subtype test runs frozen (`:4014`), so `NoVars` does.
`Universe::normalized(t)`: dealias to the applied match alias, beta-reduce (`seen_from.rs` moves
its `Lambda`→`Match` case here), reduce, repeat; cache by `TypeId` while no `Var` is in the
footprint (dotty's `reductionContext`). Hooks: `third.rs` (right `Match` → `normalized` else
fourth try), `fourth.rs:28` (left → `normalized` else bound), `applied.rs` where an applied alias
is compared (dotty `:1459`, `:1534`), `erasure/mod.rs:83` (erase the reduction, the bound as
fallback), `hk.rs` tuple element walk through `normalized`. Tests: pairs in
`scripts/subtype-oracle.py`'s corpus (`Tuple.Head[(Int, String)] =:= Int`, `Tuple.Zip`,
`Tuple.Union`, `Tuple.Elem[…, 1]`, `Tuple.Size`, a user `Elem[List[Int]]`, a stuck `Elem[T]`
with `T` abstract, a disjoint `Int` vs. `List[t]`), checked against dotc.

**M3 — the typer uses it (~150 lines).** `type_trees.rs:65-66`: keep the type (now normalizable)
instead of refusing; `typed_match_type_tree` in a new `Y/match_type_trees.rs` (bound, selector,
cases with the pattern's `t` bindings as a `Lambda` over captures, port of `T/Typer.scala:2860-2883`
and `typedTypeCase`, inferred bound as the `|` of the bodies); `Yu/admitted.rs` admits `Match` and
match aliases; the inliner's type substitution (`Yl/retype.rs`, `beta.rs`) normalizes the result
of `asInstanceOf[Head[This]]` after `This` is known, so the cast is to `Int` as in dotty's NIR;
`constValue[Tuple.Size[…]]` goes through `normalized` before `Yl/type_eval.rs`; `adapt.rs` needs
nothing, `conforms` reduces. `type_trees.rs:29-36`/`transparent.rs:71-72` lift the
`scala.compiletime.ops` refusal where the operands are ground (`Yl/type_eval.rs` folds them; a
non-ground application stays as dotty keeps it). Tests: golden inputs `mt01`–`mt05` and the six
adaptation tests in `crates/typer/tests/typed`, `just typer-compare` on each.

**M4 — `constValueTuple`, `summonAll` (~100 lines).** Both are library inline defs over
`inline erasedValue[T] match { case _: EmptyTuple => …; case _: (t *: ts) => … }`; with M2–M3 the
scrutinee type reduces and the remaining refusal is the type binding of `Yu/cases.rs:82` (a
`Lambda` over `t`, `ts` whose instances come from the inline match's type test — dotty's
`Inlines.reduceInlineMatch` with `TypeTest` patterns; ours has `Yl/reduce/` for the same, [I:
check it binds type variables for `case _: (t *: ts)`). `Yl/compiletime.rs:51-53` then drops the
refusal. Tests: constValueTuple, i15988a, summonAll, i15988b.

| MR | scope | lines | tests |
|---|---|---:|---|
| M1 cases | §4 `Type::Match` with scrutinee and cases, decoder, printers | ~120 | none new (decode of the whole library) |
| M2 reduce | §4 `match_types/` analyze, reduce, `provablyDisjoint`; `normalized`; erasure | ~350 | subtype-oracle pairs; tuples1b, toList, tuple-ops, i23897, i4600b (adaptation) |
| M3 typer | §4 match type trees, admitted inline bodies, inlined casts, ops lifted | ~150 | tuples1, i17115, i7868, typelevel-peano, enum-nat, Providers, i9530 (+ i13332a, i12328a/b, anon-mirror-gen-local after #43) |
| M4 tuple intrinsics | §4 `constValueTuple`, `summonAll`, type bindings in inline matches | ~100 | constValueTuple, i15988a, summonAll, i15988b (+ typeclass-derivation* after #43) |

Order: M1 → M2 → M3 → M4, about 720 lines for ~17 tests directly and ~12 more once #43 (mirrors)
lands; named-tuples, named-tuple-ops, named-tuples-strawman-2 and tyql need `NamedTuple`'s match
types *and* named-tuple selection (out). M1 changes a `Type` variant every crate matches on and
goes alone; M2 is the one that needs review time (`provablyDisjoint`'s sealed decomposition and
the base-type walk); M3 and M4 touch the inliner and should not run in parallel with #82 work.

Risks: a reduction that differs from dotty's reduces to a different type silently — the oracle
pairs and `typer-compare` are the guard, and an unreducible type must stay unreduced, never fall
to the bound inside the typer (only erasure may); erasure must reduce exactly where dotty does
(`translucentSuperType`), or NIR casts and unboxings differ — `just nir-cmp` on the tuple
programs; `provablyDisjoint`'s sealed-child enumeration needs the children of library sealed
classes, which the TASTy `CHILDREN` annotation (`S/symbols/`) gives [I, check it is decoded];
the `reductionContext` cache must not survive a constraint change when a `Var` is in the
scrutinee (keep no cache for those); GADT-dependent reductions (`reducesUsingGadt`) are out, as
GADTs are (ROADMAP phase 3).
