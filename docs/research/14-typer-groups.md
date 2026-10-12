# 14 — Typer groups: the MRs after inlining

Date: 2026-10-12. Claims are [V] verified by reading the code or running it, or [I] inferred.
Paths are relative to `ref/scala3` (tag 3.9.0; `T/` = `compiler/src/dotty/tools/dotc/typer/`, `C/` =
`.../dotc/core/`, `X/` = `.../dotc/transform/`, `D/` = `.../dotc/ast/`); our code at 9354f65
(branch `82-tasty-bodies`; `Y/` = `crates/typer/src/`, `S/` = `crates/tasty/src/`, `L/` =
`crates/lower/src/`). Follows [research 09](09-typer.md) and [12](12-inlining.md); ROADMAP phase 3
item 3, #83.

**Recommendation in one paragraph.** With the inlining stack in, 792 of dotty's `tests/run` still stop
at the typer (576 pass) [V]. Recording *every* refusal of a unit, not only the first, shows that 274
tests stop for one reason alone and that the cheap reasons dominate: a batch of small bugs
(sibling-block double definitions, `==` after a conversion, polymorphic overloads, `@switch`,
context bounds, secondary constructors: 45 tests with no other blocker), union widening (29), the
implicit search (37, of which a one-line fix makes `using` parameters visible to nested searches at
all), `{ case … }` literals with `PartialFunction` expected types (25), patterns needing type tests
(22). Self types and opaque types (4 run tests alone, but 196 `tests/pos` files) and higher-kinded
abstract types (0 alone, 282 pos files) are the expensive items and go last. Six MRs, each under
~1,200 lines, in the order of §3.

## 1. Where the typer stops

`just dotty-lower-report` records a test's *first* refusal (`Typer::unsupported_at`,
`Y/typer/report.rs:17-22`, keeps the first). A probe build that pushes every refusal (and rolls them
back with the constraint in speculative searches, `implicits/search.rs:101-105`,
`applicability.rs:38-43`, `implicits/can_equal.rs:73-77`) ran over the 792 refused tests
(`tmp/probe/sweep.py` → `all.tsv`, scratch in the worktree) [V]: 274 tests record one distinct
reason, 186 two, 110 three; mean 3.1. Later reasons are partly cascades of the first (an error tree
makes `member not found` 156 times, `unresolved name` 50), so "only" below (every recorded reason is
this one) is a lower bound of what fixing the reason unblocks and "first" an upper bound.

| first | only | reason | `Y/` site | group |
|---:|---:|---|---|---|
| 45 | 8 | no implicit argument | `implicits/search.rs:66` | A |
| 35 | 11 | adaptation | `typer/adapt.rs:104` | B, A |
| 27 | 10 | member not found | `typer/select.rs:59,68,114` | A, B, out |
| 25 | 0 | opaque type | `namer/checks.rs:55` | E |
| 24 | 19 | function parameter without a type | `typer/closures.rs:65` | D |
| 23 | 20 | union widening | `namer/complete.rs:95` | C |
| 20 | 0 | higher-kinded abstract type | `typer/type_lambdas.rs:16` | G |
| 20 | 10 | scala.compiletime operation | `typer/inline_refs.rs:18` | #82 |
| 18 | 10 | type pattern needing a type test or type binding | `typer/patterns.rs:33` | F |
| 17 | 4 | self type | `typer/classes.rs:51` | E |
| 17 | 3 | reference through a path | `typer/this.rs:176` | B |
| 17 | 0 | deferred, given or overriding inline definition | `namer/checks.rs:108` | #82 |
| 15 | 5 | Annotated (expression) | `typer/dispatch.rs:69` | B |
| 15 | 9 | type variable instantiated to a union | `typer/infer.rs:62` | C |
| 15 | 3 | type tree RefinedTypeTree | `typer/type_trees.rs:97` | out |
| 14 | 2 | type tree ContextBoundTypeTree | `typer/type_trees.rs:97` | B |
| 14 | 4 | Tuple (generic tuple operations) | | #39 |
| 13 | 7 | double definition | `namer/checks.rs:73`, `typer/overrides.rs:89` | B |
| 13 | 9 | overloads with dependent parameters | `typer/alternatives.rs:128` | B |
| 12 | 8 | comparison of a converted value | `typer/equality.rs:30` | B |
| 12 | 9 | secondary constructor call | `typer/apply.rs:134` | B |
| 12 | 6 | given conversion | `implicits/conversions.rs:52` | A |
| 11 | 8 | refutable pattern definition | `typer/matches.rs:104` | F |
| 10 | 4 | ambiguous implicit arguments | `implicits/search.rs:60` | A |
| 10 | 5 | block type with local symbols | `typer/stats.rs:141` | B |
| 10 | 2 | Export | | later |
| 9 | 4 | single abstract method type outside the subset | `typer/function_types.rs:50-69` | D |
| 9 | 2 | class tag of a type that is not a class | `implicits/synthesis.rs:46-48` | A |
| 9 | 1 | extractor of a type that needs a type test | `typer/unapply.rs:57` | F |

Out of scope here: `scala.compiletime` and inline definitions (37, #82), `Mirror` summons (16 of the
45 `no implicit argument`, #43), generic tuples and match types (31, #39), `Dynamic` (10, #42),
structural types (15, reflective calls), xml (3), JDK members javalib lacks (`t2849`, `t7336`,
`t8601c`; #29), `Export` (10; `T/Namer.scala:1130-1400`, a separate ~300-line MR when wanted) [V].

## 2. The groups

Sizes are Rust lines of diff [I]; "tests" are first/only from the table, added per group
(`tmp/probe/groups.py`).

### B. Definitions and applications: a bug batch (141 first / 64 only, ~800 lines)

The counts include every `adaptation` refusal (35 / 11); the conversion-related ones among them
belong to A [I].

1. **Sibling-block double definitions** (7 only): `{ val it = 1 }; { val it = 2 }` in one method is
   refused [V, repro]; `Y/namer/double_definitions.rs:12-37` compares against the *owner's* decls,
   dotty's `checkNoConflict` (`T/Namer.scala:138-160`) against the enclosing *scope*. ~40 lines.
2. **Synthetic members clashing with user ones** (`i209`: `case class Foo(a: Int) { def copy(i: Int = 9) }`)
   [V]: `invalidateIfClashingSynthetic` (`T/Namer.scala:976-1014`, names from
   `D/Desugar.scala:89`). ~60 lines.
3. **`==` after a conversion** (8 only: `xs.sum == ys.sum`) [V]: `Y/typer/equality.rs:26-31` refuses
   any synthetic `Apply` qualifier; dotty checks types only (`T/Implicits.scala:1087`
   `checkCanEqual`, `T/Typer.scala:5273` `checkEqualityEvidence`). Keep the refusal to the case
   where an *error* would have to be positioned. ~30 lines.
4. **Overloads with polymorphic alternatives** (9 only: `def f[T](x: T)` beside `def f(x: Int)`) [V]:
   `Y/typer/alternatives.rs:119-129`; `T/Applications.scala:2431-2460` (`resultConforms`
   instantiating `PolyType`s with type variables), `:2683-2730`. ~150 lines in
   `alternatives.rs`/`applicability.rs`.
5. **Expression annotations** `(x: @switch) match`, `: @unchecked` (5 only; 10 more are `for`
   generators with patterns, see F): `T/Typer.scala:3668` `typedAnnotated`. ~60 lines plus the lowering
   dropping the `Typed`.
6. **References through a path** (17 first / 3 only): `Y/typer/this.rs:170-181` builds `C.this.x`
   only; dotty's `tpd.ref`/`singleton` builds `p.q.x` from a `TermRef` prefix. ~60 lines; also
   needed by A2.
7. **Context bounds** (14 first / 2 only): `def less[T: Ordering]` and `class Foo[T: ClassTag]` are
   refused [V, repro `r37`–`r39`]: `desugar_context_bounds` (`crates/syntax/src/desugar/context_bounds.rs:44-87`)
   leaves the `ContextBoundTypeTree` as the evidence parameter's type tree, as dotty does, but
   `typedContextBoundTypeTree` (`T/Typer.scala:2663`, dispatched at `:3909`) is not ported
   (`Y/typer/type_trees.rs:96-99`). ~40 lines. Also 42 `tests/pos` files [V].
8. **Secondary constructors** (12 first / 9 only): `T/Typer.scala:3181-3219` (`checkThisConstrCall`),
   `T/Namer.scala:332` (parameters in the outer context), `T/Applications.scala:1285`
   (`thisCallArgContext`), `T/Typer.scala:749-752`; `Y/typer/apply.rs:130-137` refuses the call,
   `Y/typer/defs.rs` types the body. Lowering: `L/phases/constructors.rs` treats only the primary
   constructor; a secondary `<init>` is a method whose `this.<init>(…)` call nscplugin emits as a
   constructor call (`L/codegen/callee.rs`). ~250 lines; #83's earlier split gave it to the lowering
   owner, it is still open [V].
9. **Adaptation leftovers** (3 only): numeric literals narrowed under an instantiated type variable
   (`val bs: Array[Byte] = Array(0, 1, 2)`: `Y/typer/adapt.rs:121-141` sees the `Var`, zonk first),
   references to by-name parameters (`def bam(x: => Unit = ()) = x`; dotty `adaptNoArgs` on
   `ExprType`) [V, repros].

Risks: each item changes acceptance of programs the oracle corpus already passes; run
`just typer-report` and `snapshot-diff` per item. The remaining `adaptation` refusals (conversions
whose expected type still holds type variables, `to(LazyList)`, Java varargs with boxing) need A.

### C. Union widening (38 first / 29 only, ~600 lines)

What: inferred `Some[Int] | None.type` must become `Option[Int]`, `try` results `Int | String` must
widen to their join when the join is not transparent, `List(1, "a")` keeps `List[Int | String]`
(join `Matchable` is transparent) [V, repros `r07`, `r08`, `r18`]. 11 of the 23 `union widening`
tests are `@main` methods returning a class (`@main def Test = new A(1)` fails, `def t = new A(1)`
and a `try` under an explicit `Unit` pass): a bug in `Y/main_proxies.rs`, not widening [V].
Ports: `C/Types.scala:1454-1497` (`widenUnion`, `widenUnionWithoutNull`, `widenSingletons`),
`:3661-3714` (`OrType.join`, soft unions), `C/ConstraintHandling.scala:673-725` (`widenInferred`
with `Widen.Unions`, `hardenUnions`), `T/Namer.scala:2299`; `orDominator` is ported
(`S/subtype/join.rs`) [V]. Touches: `S/types/mod.rs` `Type::Or` gains a `soft` flag (65 match sites
across tasty, typer, lower, mechanical; TASTy `ORtype` decodes hard, `lub` makes soft,
`is_same_type` ignores the flag), `Y/namer/complete.rs:84-99`, `Y/typer/infer.rs:33-74`,
`Y/typer/widen_inferred.rs`, `Y/main_proxies.rs`. Unions already erase (`S/erasure/mod.rs:84`
`erase_or`) and the lowering handles them in type tests (`L/phases/type_tests/mod.rs:59-65`) [V].
Risks: the flag changes interner identity (`Or(a, b, soft)` ≠ `Or(a, b, hard)`), so every
comparison site must compare parts; kept unions reach the lowering more often (boxing at the erased
lub) [I].

### A. Implicit search (95 first / 36 only, ~900 lines)

1. **Parameters as candidates** (1 line): `Y/typer/implicits/candidates.rs:19-22` admits
   `Kind::Val | Kind::Def` only, so no `(using X)` or `(implicit x: X)` parameter is ever found by a
   nested search: `def f(using Foo): Int = g` with `def g(using Foo)` is refused, on `main` too
   [V, repros `r22`–`r31`]. 137 of the 792 refused tests have such clauses; with the probe patched,
   5 first refusals move on and 2 tests pass the typer (`i18183.findRef`,
   `implicit-disambiguation`), the rest hit the items below [V].
2. **Companions of inner classes** in the implicit scope: `Y/typer/implicits/scope.rs:100-109` returns
   none for a class nested in a class (`c.Inner` with `object Inner { implicit def … }`,
   `test-implicits`) [V]; dotty `collectCompanions` (`T/Implicits.scala:706`) goes through the
   prefix. Needs B6 for the reference. ~60 lines.
3. **`given Conversion[A, B]` and implicit function values as views** (12 first / 6 only, plus 2):
   `conversions.rs:50-57` refuses them; dotty applies `conv.apply(x)` (`T/Implicits.scala:1184-1240`
   `typedImplicit`, `tryConversion`); the lambda form `given Conversion[Int, String] = _.toString`
   also needs D's SAM of an abstract class [V, `r21`]. ~120 lines.
4. **Function-typed implicit arguments** (`flatten`, `transpose`: `implicit ev: A => IterableOnce[B]`,
   satisfied by `Predef.$conforms`/`<:<`; 5 tests) [V, `r01`]. The search must try a
   `ViewProto`-like candidate set for function expected types (`T/Implicits.scala:948-1014`). ~80 lines.
5. **Ranking** (10 first / 4 only, #81): `T/Implicits.scala:1370-1467` `compareAlternatives` with
   nesting levels and `T/Applications.scala:2159` `compare`. ~200 lines in `implicits/ranking.rs`.
6. **By-name implicits** (`byname-implicits-*`, 3): `T/Implicits.scala:1981-2060`. ~100 lines.
7. **`ClassTag` for applied and array types** (9 first / 2 only): `synthesis.rs:40-49`;
   `T/Synthesizer.scala:32-95` `synthesizedClassTag`. ~60 lines.

Risks: ranking and new candidates change which implicit wins silently (oracle corpus, `tests/neg`
parity through `typer-stress`); more candidates per search cost time (#48, `MAX_TRIES`).

### D. `{ case … }` literals and SAM expansion (37 first / 25 only, ~700 lines)

20 of the 24 `function parameter without a type` tests are pattern-matching anonymous functions
against a `PartialFunction` expected type (`collect { case … }`, `val pf: PartialFunction[Int, Int] =
{ case x => x }`) [V, `r12`]; the SAM ones are lambdas to abstract classes or traits with state
(`Conversion`, `MyFunction0`) [V]. Ports: `T/Typer.scala:2181-2198` (`typedMatch` without selector →
`makeCaseLambda`, `D/Desugar.scala:2004`, formals from `decomposeProtoFunction`),
`:2132-2179` (`typedClosure`: `PartialFunctionOf` when the expected type is underspecified),
`:1857-1990` (`typedFunctionValue`); lowering `X/ExpandSAMs.scala:49-240` (`toPartialFunction`:
an anonymous `AbstractPartialFunction` subclass with `isDefinedAt`/`applyOrElse` from two copies of
the match; the general case: an anonymous class for a non-platform SAM). Touches:
`Y/typer/closures.rs:21-69`, `Y/typer/function_types.rs:38-70` (`sam_parts`), `Y/typer/matches.rs:80-90`,
`L/phases/closures.rs:52` (refuses closures of non-function types), a new `L/phases/expand_sams.rs`
right after FirstTransform (before patmat and LambdaLift, as in dotty), `L/codegen/closure.rs`.
Size: typer ~250, lowering ~450. Risks: the anonymous class is made after typing (owner, outer and
captured variables; dotty's order avoids LambdaLift seeing it); the class's NIR name and members
must match nscplugin's byte for byte (`just nir-cmp`) [I].

### F. Patterns needing type tests (60 first / 22 only, ~800 lines)

`case _: A[_]`, `case _: x.type`, `case _: "a"`, `case _: T` for an abstract `T`
(`Y/typer/patterns.rs:30-34`), extractors whose scrutinee is a supertype of the unapply parameter
(`unapply.rs:57`, 57 tests mention it), refutable pattern definitions `val List(a, b*) = …`
(`matches.rs:92-114`), named or typed subpatterns, `unapplySeq` result types. Ports:
`X/TypeTestsCasts.scala:185-330` (`interceptTypeApply`, `isCheckable`, `transformTypeTest`:
wildcard-applied, singleton via `eq`, literal, unchecked warnings), `:388` `checkTypePattern`;
`T/Checking.scala:1073-1120` `checkIrrefutable`, `D/Desugar.scala:1505` `makePatDef`;
`T/Applications.scala:1718` `typedUnApply` (the scrutinee cast before `unapply`). Touches:
`Y/typer/patterns.rs`, `unapply.rs:25-65`, `unapply_seq.rs`, `matches.rs`, `L/phases/type_tests/`,
`L/phases/patmat`. Size: typer ~450, lowering ~350. Risks: dotty's `unchecked` warnings are part
of the oracle output; type-variable binding in patterns stays out (#41) [I]. The 10 `for`
generators with patterns (`Annotated` in the table) fall here too.

### E. Self types and opaque types (42 first / 4 only; 95 + 101 pos files, ~1,100 lines)

Opaque types block no run test alone, because every use of the alias cascades into `adaptation`,
`given conversion` or `member not found` once the alias is unknown [V]; in `tests/pos` they are the
third most frequent first refusal (101), self types the fifth (95) [V]. Both hang on `ClassInfo`'s
self type: dotty stores an opaque alias as a refinement of the owning object's self type. Ports:
`T/Namer.scala:1632-1646` (`selfInfo`, `inClassContext`), `C/Types.scala:5594-5609`
(`ClassInfo.selfType` = given self `&` applied class), `C/SymDenotations.scala:2003`
(`givenSelfType`), `C/Types.scala:3144` (`This` underlying), `T/Typer.scala:3351-3367`
(`classExistsOnSelf`); opaque: `T/Namer.scala:1178-1191`, `C/SymDenotations.scala:444-474`
(`opaqueToBounds`), `:1398-1410` (`opaqueAlias`), `C/TypeComparer.scala:1611-1619`
(`tryLiftedToThis`), `:1717-1740` (`liftToThis`), `T/Typer.scala:832-857` (`tryLiftToThis` in
`typedSelect`), `C/Types.scala:1589-1611` (`dealiasKeepOpaques`), `T/Typer.scala:3255`. Touches:
`S/symbols` (a self type per class, `self_type`), `S/subtype` (lifting prefixes to the `This` of an
enclosing opaque-containing object), `S/types/widen.rs:101-105` (library opaque types stay
abstract), `Y/typer/classes.rs:48-52`, `Y/namer/checks.rs:54-55`, `Y/typer/this.rs`,
`Y/typer/members.rs`, `Y/typer/select.rs`; the lowering erases self types away and script opaque
types to their alias (`S/erasure`). Size: self ~350, opaque ~550, tasty ~200; two MRs if the second
grows. Risks: `This` with an underlying self type changes member lookup in every class;
`liftToThis` depends on the context owner, so either `is_subtype` learns the owner or the typer
lifts prefixes before comparing [I].

### G. Higher-kinded abstract types (32 first / 0 only; 282 pos files, two MRs)

`type F[_]` members, `F[_]` parameters, `type M[X] <: …` with variance; every run test that hits it
hits two to six more HK reasons (`wrong number of type arguments` 45, `type argument of another
kind`, `polymorphic method overriding a member`) [V]. Ports: `T/Namer.scala:1124-1195` (`typeSig`,
`abstracted` = `HKTypeLambda.boundsFromParams` `C/Types.scala:4598`, `addVariances`),
`T/Typer.scala:2849-2852`, `:2893-2915`; `C/TypeApplications.scala:206-520` (`typeParams`,
`hkResult` :305, `etaExpand` :355, `appliedTo` :410, `toBounds` :506); `C/TypeComparer.scala:745-760`,
`:1067-1080` (`HKTypeLambda` cases), `:1250-1290` (applied types with lambda constructors),
`:2804-2830` (`liftIfHK`). Touches: `Y/typer/type_lambdas.rs:14-64`, `type_defs.rs`,
`type_apply.rs`, `variances.rs`, `S/subtype/hk.rs`, `S/subtype/applied.rs`, `S/types/decode`.
Size: 7a abstract HK members and parameters of simple kinds (~900), 7b variance lambdas, bounds
referring to their parameters, polymorphic overriding (~600). Risks: the subtyper's rules for
applied types change for every type; little run-test payoff until the follow-on reasons go [I].

## 3. Order

By tests unblocked per line, "only" counts first [I]:

1. **B** bug batch: 64 only / 141 first, ~800 lines. Items 1, 3, 4, 7 and the `@main` fix of C
   are each under 60 lines and worth a first small MR of their own.
2. **C** union widening: 29 / 38, ~600; the `soft` flag is the one design decision (ADR).
3. **A** implicits: 36 / 95, ~900; start with item 1 (one line, measured) and 2–4.
4. **D** `{ case … }` and SAMs: 25 / 37, ~700, half of it lowering; also what `given Conversion =
   lambda` and A3 need.
5. **F** patterns: 22 / 60, ~800, half of it lowering.
6. **E** self and opaque types: 4 / 42 run, 196 pos, ~1,100; before any script with an opaque type
   or a `self =>` trait can run.
7. **G** higher-kinded abstract types: 0 / 32 run, 282 pos; two MRs, when typeclass-style scripts
   are the target.

Together groups B–E are the first refusal of 413 of the 792 tests and the only recorded refusal
of 215 (445 and 218 with G) [V]; the rest is #82, #43, #39, out-of-subset features, and the 59
multi-file and 31 VM-failing tests the report skips.
