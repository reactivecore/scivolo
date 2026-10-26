# 38 — Refusals in dotty's tests/run and tests/run-macros after 2026-10-24

Date: 2026-10-26. Claims are [V] verified by reading the code or running it, [I] inferred. Paths
as in research 37: `T/` = `ref/scala3/compiler/src/dotty/tools/dotc/typer/`, `C/` = `.../dotc/core/`,
`X/` = `.../dotc/transform/`, `Y/` = `crates/typer/src/typer/`, `Yn/` = `crates/typer/src/namer/`,
`Yi/` = `crates/typer/src/inlining/`, `Yu/` = `crates/typer/src/unpickle/`, `L/` =
`crates/lower/src/phases/`, `M/` = `crates/macros/src/`. Measured on main 085ab8e1 (after the
2026-10-24 stack and stdlib-g3) with `just dotty-lower-report` (`tmp/dotty/3.9.0-0.5.12/lower-results.tsv`):
**1321 pass** of 1711 (research 37: 1263), 160 typer refusals in 80 distinct texts, 10 lowering,
14 parser, 1 wrong output (sticky-extmethod), 32 fail in the VM also under Scala Native, 173
skipped [V]. `just dotty-lower-report "" run-macros` (`lower-results-run-macros.tsv`): 321 tests,
**188 pass** (research 37: 134), 119 typer refusals in 68 texts, 2 lowering, 3 frontend crashes,
1 earlier group, 8 skipped [V]. Against the main checkout's results of 2026-10-23 (1233 pass): 88
tests improved to pass, none worsened, 9 moved from a typer to a lowering refusal [V diff].
`just typer-report` (tests/pos, for the record): **1893 pass**, 152 differ, 22 warnings differ,
957 unsupported, 1 crash of 3025; tests/programs 68 pass, 14 differ, 3 unsupported [V].
`just lib-compile all` (#168; oracle: the published jars, no `tmp/lib/sn` in this checkout, so the
NIR counts are not ROADMAP's): nativelib 51 of 103 files lowered, clib 17 of 21, posixlib 52 of 67,
windowslib 21 of 34, javalib 338 of 601, auxlib 6 of 15 [V].
Method: texts grouped by their site (grep of the text, `tmp/r38-sites.txt`); the refusing source
line of each of the 170 single-file refused tests of `tests/run` from `scivolo-typer-dump`
(`tmp/r38-run-attrib.tsv`), the 14 directory tests through `scivolo-lower` on their files
(`tmp/r38-dirs.py`); the grouping of all 185 stops checked for coverage (`tmp/r38-groups.txt`) [V].

**Recommendation.** 61 of the 185 frontend stops in `tests/run` belong to the four running
tracks or to closed areas (§1.1); of the rest, six batches with disjoint files and features (§4):
A types and kinds (Selectable block types, refinements, structural calls, closures, higher-kinded
aliases; 24 tests, ~300 lines), B implicits (scope through instance prefixes, extension methods of
conditional givens, class tags of tuples, search shapes; 30 tests, ~400), C match types, patterns
and mirrors (15 tests, ~300), E adaptation and application singles plus namer, overrides and
references (34 tests, probes first), F erased definitions (21 tests, ~300, experimental, last), G
macros driven by `tests/run-macros` (13 tests of `tests/run` and 125 stops of `run-macros`). The
open items of the TODO issues are folded into the batches (§3); sticky-extmethod, the one wrong
output, is B's first item. Done on main: #60, #89, #90, #139, #150, #152, #159, #160, #161, #162.

## 1. Attribution

### 1.1 tests/run: 185 frontend stops

| where | tests | names |
|---|---:|---|
| running: stdlib-javalib (G6c, G6d) | 3 | enums-thunk (`Y/apply.rs:115` "result type of a wildcard argument"), t2849, t7336 (javalib 0.5.12 has no `Runtime.totalMemory`) |
| running: lowering #165 (`L/`, non-macro `Yi/`) | 19 | its features: typeclass-derivation2b (adaptation at `cas.toProduct(x.asInstanceOf[cas.Prod])`), implicit-alias (`L/local_lazy.rs:91`), i13332 (`L/local_free.rs:114`), i23279 (local objects in an inline body, "member not found" at `defn.Lifecycle.FromZIO`); its files only: ift-return (`L/context_results.rs:84` "methods with context function results"), publicInBinary (`L/protected_accessors.rs:135`), i12328a, i13332anonprod, i13332shapeless-b (`L/outer_args.rs:109` "creating an inner class through this prefix"), statics, i5924c (`L/static_members.rs:73` "@static values"), i5924b (`:32`), and the inliner singles i12914, i13461-b (`Yi/opaque_proxies.rs:40,49`), named-tuples-xxl, tuple-map (`Yi/inline_call.rs:82`), i20095 (`Yi/beta.rs:151`), i6858 (`Yi/proxies.rs:34`), erased-inline-product-1 (`Yi/reduce/bindings.rs:47`) |
| macros (research 26; batch G) | 13 | `typeChecks`/`typeCheckErrors` (research 26 C): i11630, i12656, i18150, i22968, i7487, i7488, scala-tests-typeChecks, scala2-library-test, typeCheckErrors; i8530, i8577i ("transparent inline extractor", `Y/unapply.rs:65`, `Y/transparent.rs:75`); quoted-sematics-1 (adaptation at `case (Nat(n), PNat(n2))`); i22649 ("unresolved name T", the `TypeReprStructure` host, W5) |
| #41 GADT | 4 | gadts, i14983, i16785 (`Y/unapply.rs:251`), enum-Tree (adaptation at `case True => true` in `eval[T](e: Tree[T]): T`: `T := Boolean` from the case's parent) |
| #127 type tests | 4 | fully-abstract-nat ("member not found" `n.value` on a pattern-bound abstract type), -4, -6, -7 (adaptation of `Succ.unapply`'s `Refined` results) |
| #147 divergence measure | 2 | byname-implicits-17, generic ("no implicit argument") |
| out: XML, `into`, `3.3`, spreads, modularity, capture checking, package object values, `@specialized(AnyRef)`, `caps.unsafe` | 16 | i16458, i16459, xml; convertible; drop-apply-optimization; i11419, spreads-subtyping, spread-patterns, spreads; i15840, i3920, given-disambiguation; errorhandling; pkgobjvals; t4770; i16943 |

The other 124 split into §2's groups: A 18 + D 6, B 30, C 15, E 21 + N 13, F 21 (11 of them
parser refusals); the 13 of G are counted above [V `tmp/r38-check.py`]. Eight of the lowering
track's 19 are outside #165's feature list (ift-return, publicInBinary, the three
`L/outer_args.rs` tests, the three `@static` ones): the coordinator may add them to that track
or hold them for the next wave; no batch here edits `L/`.

### 1.2 tests/run-macros: 125 stops

| where | tests | names |
|---|---:|---|
| a reflect method the overlay lacks ("the VM cannot run: no method") | 29 | `SuperTypeTest` 12 (expr-map-1/-2/-3, flops-rewrite ×3, i17445 ×3, quote-matcher-symantics-2, quote-matching-open, tasty-extractors-owners), `SymbolMethods` 4 (i12351, i14902, i16734c, i7025b), `TypeLambdaTypeTest` 2 (i16734a, -b), `CompilationInfo` 2 (is-in-typer, Xmacro-settings-simple), `TypeReprMethods` 2 (i8514b, TypeRepr-isTupleN), `TypeBoundsTreeTypeTest`, `ThisTypeTypeTest`, `MatchTypeTest`, `GivenSelector`, `ClassDefMethods` (i6988), `Class.getCanonicalName` (inline-case-objects, javalib), `Macro$.case1` (macro-erased, an erased parameter) |
| a reflect operation refused by name | 43 | `Symbol.tree` of a symbol outside the unit 4, `Symbol.newClass` 3 (traits, type parameters), `Printer.TypeReprStructure` 3, `Type`/`TypeRepr.toString` 3, `TypeRepr.show` of an unprinted type 3, `MethodType.*` 3, `Expr.toString` 2, `Symbol.toString` 2, `Implicits.searchIgnoring` 2, `Select.overloaded` without one most specific alternative 2, `codeOf` of an unprinted argument 2, an operation on `Symbol.noSymbol` 2, singles: `ConstantType.apply`, `defn.AnyClass`, `defn.StringClass`, `Flags.Deferred`, `Printer.TreeAnsiCode`, `Symbol.children` of a library class, `Symbol.methodMembers` of a non-class, `Symbol.newBind`, `Symbol.newVal` with other flags, `Symbol.requiredModule`, `TypeRepr.substituteTypes`, annotations of a symbol outside the unit |
| splices and quote patterns (research 26 P, B) | 8 | a splice other than a macro call 3 (quote-and-splice, tasty-getfile-implicit-by-name-fun-context, tasty-implicit-fun-context-2), a pattern selecting from a hole 2 (i8745, i8745b), i10464, i17105 (higher-order hole of a polymorphic method), inline-varargs-1 (a macro argument other than a quote) |
| macro annotations (ADR 0039) | 4 | annot-add-global-class, annot-gen2, annot-generate (in a quote or library inline body), annot-mod-class-add-local-class (lowering: "calls of methods outside classes") |
| frontend crashes: trees the macro inspects differ | 3 | reflect-typeChecks (`typeChecks` intrinsic, research 26 C), tasty-interpolation-1, xml-interpolation-2 ("Expected statically known StringContext": the `StringContext.apply` tree of an interpolation [I shape]) |
| tooling | 2 | Xmacro-settings-compileTimeEnv (scala-cli lacks `-Xmacro-settings` for the earlier group), annot-java-tree (`SuppressWarnings`, a JDK stub) |
| §2's groups, counted here | 36 | adaptation 7 (beta-reduce-inline-result, enum-nat-macro, paramSymss, quote-matcher-symantics-3, quote-whitebox, refined-selectable-macro, whitebox-inline-macro), erased 4 (i12021, reflect-isFunctionType, i17105b, term-show), library inline bodies 4 (quote-match-poly-function-1, -1-regression, -2: `Yu/locals.rs:87`; i15968: `Yu/selections.rs:111`), "member not found" 3 (f-interpolator-tests, i10011, refined-apply), opaque types 3 (i18283, tasty-dealiasKeepOpaques top-level; opaque-inline "implicit scope of an abstract or opaque type with a companion"), "local type escaping" 2 (quote-matcher-runtime, tasty-tree-map), mirrors 2 (quoted-ToExpr-derivation-macro "Mirror of a singleton outside the subset", quoted-liftable-derivation-macro library sealed class), inline accessors 2 (i4803c local class, i12352 polymorphic or implicit method), lowering i10880 ("parameters of library traits"), singles: type-print (compile-time operation type), BigFloat (generic number literal), tasty-simplified (wildcard HK argument), exports (wildcard export from a non-class path), lib-StringContext (macro definition), reflect-inline (compiletime operation), tasty-overload-secondargs (ambiguous overload), tasty-seal-method (`Apply` arity) |

## 2. Groups, ranked by tests per line

| § | group | batch | tests | lines | scivolo | dotty |
|---|---|---|---:|---:|---|---|
| 2.1 | implicit scope through instance prefixes | B | 6 | ~40 | `Y/implicits/scope.rs:41` | `T/Implicits.scala:706-770` `collectCompanions` |
| 2.2 | class tags of tuples and from scope | B | 3 | ~20 | `Y/implicits/class_tags.rs:43,72` | `T/Synthesizer.scala:32-93` |
| 2.3 | abstract redeclarations and inherited conflicts | N | 3 | ~30 | `Y/overrides.rs:136`, `Y/inherited_conflicts.rs:38` | `T/RefChecks.scala:274-278` |
| 2.4 | `Selectable` anonymous classes in block types (#122) | A | 4 | ~40 | `Y/class_bound.rs:32-34`, `Y/stats.rs:154` | `C/TypeOps.scala:422` `classBound` |
| 2.5 | refinements and structural calls | A | 8 | ~120 | `Y/refined_types.rs:48,62`, `Y/refinement_members.rs:85`, `Y/structural.rs:81` | `T/Typer.scala:2704`, `T/Dynamic.scala:125,184` |
| 2.6 | extension methods of conditional givens and refined values | B | 6 | ~80 | `Y/implicits/extension_views.rs:29,68`, `view_kinds.rs:80` | `T/Implicits.scala:1184`, `T/Typer.scala:4307` |
| 2.8 | match types and type patterns | C | 11 | ~200 | `Y/adapt.rs:129` (dependent matches), `Y/type_trees.rs:82,134`, `Y/matches.rs:100`, `Y/unapply.rs:256`, `Y/patterns.rs:43` | `T/Typer.scala:2299`, `T/Applications.scala:1718,1763` |
| 2.12 | namer, literals, references | N | 10 | ~150 | `Y/find_ref.rs:138`, `Yn/params.rs:125`, `Yn/checks.rs:54`, `Yn/export_paths.rs:21`, `Yn/inline_checks.rs:32`, `Y/literals.rs:45`, `Y/defs.rs:37` | `T/Typer.scala:220-432` `findRef`, `:1165`, `T/Namer.scala:1178`, `C/TypeErasure.scala` |
| 2.7 | implicit search shapes (probe first) | B | 15 | ~250 | `Y/implicits/{unsettled,clauses,search,conversions,dictionary,adapt_by_result}.rs` | `T/Implicits.scala`, `T/Synthesizer.scala:795` |
| 2.10 | kinds, closures and type singles | A | 12 | ~180 | `Y/type_lambdas.rs:78`, `Y/type_trees.rs:145`, `Y/type_args.rs:23`, `Y/call_types.rs:28`, `Y/closures.rs:64,68`, `Y/constructors.rs:37,98`, `Y/select.rs:136` | `T/Namer.scala:1668` `parentType`, `T/Typer.scala:1857,2097` |
| 2.9 | mirrors (#129, #120) | C | 4 | ~100 | `Y/implicits/sum_children.rs:18`, `product_mirrors.rs:66`, `Y/deriving.rs:22` | `T/Synthesizer.scala:681`, `T/Deriving.scala:26` |
| 2.11 | adaptation and application singles (probe first) | E | 21 | — | `Y/adapt.rs:129`, `Y/apply*.rs`, `Y/eta.rs:186`, `Y/named_args.rs:40,181`, `Y/infer.rs:175,180` | — |
| 2.13 | erased definitions and `saferExceptions` | F | 21 | ~300 | `crates/syntax/src/parse/`, `Yn/params.rs:119`, `Y/function_types.rs:143,168`, `Y/constructors.rs:57` | `T/Typer.scala:1795-1828`, `X/Erasure.scala` |
| 2.14 | macros | G | 13 + 125 | — | `M/`, `overlay/`, `Yi/reflect*`, `Yi/quote*`, `Yi/splice.rs`, `Yi/macro_*` | `Qi/QuotesImpl.scala` |

### 2.1 Implicit scope through instance prefixes (6 tests, ~40 lines)

`Y/implicits/scope.rs:41` refuses the implicit scope of a class whose base class is a member of
another class's instance (the prefix is a path, not a static owner): anon-mirror-gen-local
(`summon[Mirror.Of[o.Item & o.Wrap.Fruit]]`), fully-abstract-nat-5 (`def refined = this` in
`SuccClass` whose bases are members of an enclosing object), i11174 (`summonFrom` inside an inline
method, research 37 listed it under `Yi/host.rs`; the host now searches and stops here), i13332a
(`Mirror.Of[First.Nested.Lst]`), typealias_overriding (`removing.printNode(new Node, …)` on an
inner class of a path) [V dumps]. dotty's `collectCompanions` (`T/Implicits.scala:706`) keeps the
prefix: `addCompanion(pre, companion)` adds `TermRef(pre, companion)` for the class and, through
`iscopeRefs(p)` of `t.parents` (`:748`), for every base class seen from the same prefix; `addPath`
(`:755`) adds the path's own members as implicits [V]. Port: in `implicit_scope_of`, replace the
guard by the prefix-aware companion reference (`Type::Sym(Some(prefix_as_seen), module)`) that the
`term_path` branch below it already builds for `base == c` [I]. deriving-from-product-typed
("member not found" at `ma.fromProductTyped(…)`, `ma: Mirror.ProductOf[B]`) is the same
mechanism [I]: `ProductOf[B]` is a refinement of `Mirror.Product`, a member of the object `Mirror`
whose extension methods dotty's `addPath(pre)` puts into the scope of every type selected from it.

### 2.2 Class tags of tuples and from scope (3 tests, ~20 lines)

i22345 (`Array[Int *: String *: EmptyTuple]()`, "class tag of a type erasing to no class",
`class_tags.rs:72`), i22150 (`IArray((dx = 0, dy = 1), …)`, a named tuple, "without a stable
erasure", `:43`), i13433b (`case res: Matcher[A]` with `(using tt: ClassTag[Matcher[A]])` in
scope, `:43`) [V dumps]. dotty's `synthesizedClassTag` (`T/Synthesizer.scala:32-93`) erases
`tp.normalizedTupleType` (`:80`: `*:` chains and named tuples become `TupleN`) and is a fallback
only: a `ClassTag` given in scope is found by the normal search first [V]. Port: normalize tuples
before `erased_class_type`, and let `class_tags.rs:43` return "not synthesized" instead of
refusing when the argument's erasure is unstable, so the scope's given wins.

### 2.3 Abstract redeclarations and inherited conflicts (3 tests, ~30 lines)

i7359 (`trait ObjectInterface: def equals(obj: Any): Boolean …`) and CollectionTests (`sealed
trait List[+A] extends Seq[A] … { def isEmpty: Boolean }`) stop at `Y/overrides.rs:136` "abstract
member overriding a concrete one" [V dumps]. dotty's `OverridingPairsChecker.checkAll`
(`T/RefChecks.scala:274-278`) runs `checkOverride(dcl, other)` for a deferred `dcl` over a
concrete `other`, which passes when the types match: an abstract redeclaration of a concrete
inherited member is legal and the concrete member stays the implementation [V]. Ours refuses
whenever the abstract member overrides concrete members only (`overrides_only_concretely`); the
2026-10-24 commit "Refuse abstract redeclarations of Object's methods with other result types"
kept that guard. Accept matching types, refuse differing ones. productElementName
(`Y/inherited_conflicts.rs:38`, #154's leftover: the synthetic `productElementName` beside an
inherited concrete one) belongs to the same file set.

### 2.4 `Selectable` anonymous classes in block types (4 tests, ~40 lines)

Unchanged from research 37 §2.2: i13703, i14340, i24056, selectable-new; `Y/class_bound.rs:32-34`
returns `None` for a term member that needs a refinement, so `Y/stats.rs:154` refuses [V]. dotty's
`classBound` (`C/TypeOps.scala:422-459`) refines the parents with term members too. Research 37's
batch A did dependent function types and refinements adding type members instead; this stays open.

### 2.5 Refinements and structural calls (8 tests, ~120 lines)

- "structural refinement of an existing member" (`Y/refined_types.rs:48`): i8073, i8073b `CC { val
  a: aa.type }` narrows an existing `val` to a singleton; dotty's `typedRefinedTypeTree`
  (`T/Typer.scala:2704`) accepts any refinement a parent member admits [V].
- "refinement referring to the refined type's members" (`:62`): i23409 `Ops[F, A] { type T =
  Func[F] }`, typeclass-derivation2a `Generic[Lst[T]] { type Shape = ShapeOf[T] }` — the
  right-hand sides mention no refinement member, only outer type parameters; `refers_to_refinement`
  is too coarse [I]. Research 37 §2.6's port note applies.
- "structural refinement with a context parameter" (`Y/refinement_members.rs:85`): structural,
  structural-compat `def fun4(implicit x: Int): Int`; dotty's `handleStructural`
  (`T/Dynamic.scala:184`) passes implicit arguments like the others to `applyDynamic` [V].
- "structural call on a Selectable other than reflect.Selectable" (`Y/structural.rs:81`): i17761,
  structural-contextual define their own `selectDynamic`/`applyDynamic`; dotty's `typedDynamicSelect`
  (`T/Dynamic.scala:125`) calls whatever `Selectable` the receiver is [V].

### 2.6 Extension methods of conditional givens and refined implicit values (6 tests, ~80 lines)

extmethods2 (`List("abc").foo(…)`), extension-specificity2, fragables-extension stop at
`Y/implicits/extension_views.rs:68` "extension method of an implicit method": the candidate is a
conditional given (`given [T](using …): Foo[T]`) whose value exists only after its implicit
arguments are found [V dumps]. dotty's `typedImplicit` (`T/Implicits.scala:1184`) adapts the
candidate against the `SelectionProto`, applying its implicit arguments, before
`tryExtensionOrConversion` (`T/Typer.scala:4307`) selects the method [V]. i9928 (`:29` "ambiguous
extension methods of implicit values": two givens offer `read`, one imported from a lower
priority object) needs the ranking of `most_specific` on the containers [I]. byname-implicits-6
(`dhh(r.head)` with `dhh: DeepHLister.Aux[H, OutH]`, a refined type) and i23897
(`s.combine(…)` with `s: Semigroup[m.MirroredElemTypes]`) stop at `view_kinds.rs:80` "implicit
value of a type other than a class": `extension_members` asks for the class symbol of the widened
type; a refined or abstract-typed value has members through its parent or bound [V site].

### 2.7 Implicit search shapes (15 tests, ~250 lines, probe first)

i13986, i13987 (`unsettled.rs:29`: `convertMu[X[x] <: Xa[x], A, B]` against `Mu[Long]`, a
higher-kinded variable instantiated by the expected type), i14020, implied-priority
(`clauses.rs:56` "nested implicit argument left to its default"), implied-specifity-2 (`given
reduced(using ev: Arg2 | Low)`: a union-typed using parameter, "no implicit argument"),
tupled-function-untupled (`f0.untupled`, `synthesizedTupledFunction`), manifest-summoning (#166
item 3, `T/Synthesizer.scala:795`), byname-implicits-28 (`dictionary.rs:89`), Providers and
enrich-gentraversable (`search.rs:109` "ambiguous implicit arguments": local givens defined later
in the block; `IsIterable[Repr]`), i3539 and t8280 (`conversions.rs:83` ambiguous conversions,
#166 item 4; `println(5: String)` ranks conversions by specificity), tagless
(`adapt_by_result.rs:53`), iterator-from (`search.rs:72`, #166 item 4),
string-context-implicits-with-conversion (`given [T] => (show: Show[T]) => Conversion[T, Showed]`,
a conversion with a using clause, applied to the repeated arguments of an extension on
`StringContext`) [V dumps]. Research 28's method: `typer-compare` probe per test before coding.

### 2.8 Match types and type patterns (11 tests, ~200 lines)

- Dependent matches: i10511 (`def not[B <: Bool & Singleton](b: B): Not[B] = b match { case b:
  True.type => False … }`), i10930 (`def leafElem[X](x: X): LeafElem[X] = x.asMatchable match …`)
  stop at `Y/adapt.rs:129` typing a case body against the unreduced match type [V dumps]. dotty's
  `typedDependentMatchFinish` (`T/Typer.scala:2299`) types each case against the match type's
  case result when the scrutinee's type matches the pattern [V]. Lives in `Y/matches.rs` and
  `Y/match_type_trees.rs`, not in `adapt.rs`.
- "parameterized type alias applied to a wildcard" (`Y/type_trees.rs:82`): 9416, erased-inline-product-2
  (`sizeProduct[(p & Product)]`) — `reduces_with_wildcards` says no for a match-type alias [V site].
- Pattern-bound type variables (#127 item 2, #164 §2.2): i15302a (`case bs: *:[bh, bt]` in a
  non-inline match, `Y/matches.rs:100` "case type with pattern variables"), i5077 (`Y/unapply.rs:256`
  "pattern-bound type against a wildcard argument"), typelevel-peano (`inline constValue[N] match {
  case _: S[n1] => … }`, `Y/patterns.rs:43`) [V dumps]. dotty: `typedUnApply`
  (`T/Applications.scala:1718`) and `typedTyped` (`T/Typer.scala:1289`) bind the variables through
  `constrainPatternType` [V names].
- "type tree Apply" (`Y/type_trees.rs:134`): i13332shapeless (`case h *: tl => h :+: CListRefl[tl]`),
  named-tuples-strawman-2 (`case true => N *: IndicesWhereHelper[t, P, S[N]]`): infix type
  applications in match-type case bodies reach the type-tree typer as `Apply` [V dumps, I cause].
- i17115 (`given inductive: [Tup <: NonEmptyTuple] => A[Tuple.Tail[Tup]] => A[Tup]`, "no implicit
  argument"): `Tuple.Tail[(String, String, String)]` must reduce during the search [I]; the
  reduction is C's (`crates/tasty/src/subtype/match_types/`), the search B's.
- caseclasses (`def Abs(x: Int) = …; Abs(2) match { case Abs(4) => }`, "member not found"): the
  extractor name resolves to the method; dotty's `followTypeAlias` in `typedUnApply`
  (`T/Applications.scala:1763`) retries the name as a type and takes the class's companion [V].

### 2.9 Mirrors (4 tests, ~100 lines)

i14540-priorRun (`Y/implicits/sum_children.rs:18` "sum Mirror of a library's sealed class":
children not decoded from TASTy, #120 item 3, #129 item 3; also macros
quoted-liftable-derivation-macro), unroll-multiple (`product_mirrors.rs:66` `@unroll`),
typeclass-derivation3 (`Y/deriving.rs:22` "derives clause of an object": `case object Thing
derives Eq, Pickler, Show`; dotty's `Deriver` (`T/Deriving.scala:26`) derives for the object's
singleton type), poly-kinded-derives (`Mirror { type MirroredType[X[_]] = F[X]; type
MirroredElemTypes[_[_]] }`, "no implicit argument": a higher-kinded mirror,
`T/Synthesizer.scala:681` `synthesizedMirror`) [V dumps].

### 2.10 Kinds, closures and type singles (12 tests, ~180 lines)

Kinds and named tuples (6): decorators, i22888 (`Y/type_lambdas.rs:78` "higher-kinded type alias overriding a type member":
`type A[T] = Int` over `type A[T]`), tyql (`Y/type_trees.rs:145` "type constructor without
arguments": `case class Select[A](…) extends Expr` with `trait Expr[Result]`; dotty's `parentType`
(`T/Namer.scala:1668`) infers a parent's missing type arguments), i15998
(`extension[A, CC[B] <: SeqOps[B, CC, CC[B]]](ring: CC[A])`, `Y/type_args.rs:23` "wrong number of
type arguments" on the F-bounded higher-kinded parameter), i19629 (a polymorphic function value
`[F1[_], Y] => (k: Knit[CP, F1]) => (ev: TypeEqK[Option, …]) => 42` against a dependent
parameter type, adaptation; `T/Typer.scala:2097` `typedPolyFunctionValue`), namedtuples-buildcolumn
(#146, `Y/call_types.rs:28`) [V dumps]. Research 37 §2.12's other tests pass now [V].
Closures (3): 16405 (`argumentTypesOf(exampleFn(_, _))` against `(?, ?) => ?`,
`Y/closures.rs:64` "parameter type inferred from the target of a wildcard"), break-opt and
loops-alt (`boundary: lab1 ?=>`, `:68` "context closure parameter of an open type": the label's
type is settled by the body; dotty's `typedFunctionValue` (`T/Typer.scala:1857`) types the
parameter with the formal and lets inference close it) [V dumps]. Type singles (3): t7985b
(`case _: Array[a#X]`, `Y/select.rs:136` "projection of a type alias"), i4557 (`class DT21 extends
T21[Int](2, "c")` with `type T21[T] = C2[T, String]`, `Y/constructors.rs:37` "new of a
parameterized type alias"), i13332intersection (`case class JsonArray[B <: Json[Any]] … extends
JsonCompound[List[B]]` with `export items.apply`, `:98` "type parameter bounds referring to type
parameters": bounds seen through a `this` prefix count as dependent) [V dumps, I causes].

### 2.11 Adaptation and application singles (21 tests, probe first)

`Y/adapt.rs:129`: fully-abstract-interface (#155 leftover), i9530 and instances-anonymous and
instances (#166 item 1: extension methods of a context-bound given, `given listOrd: [T: Ord] =>
Ord[List[T]]`), inline-numeric (`new ReverseOrdering(f) {}` in an inline method, an anonymous
class of a private parameterized trait), lst (`newElems(i) = op(elem(i))` in `inline def map`),
transparentArrowAssoc (`Numeric.BigIntIsIntegral -> Ordering.BigInt` into `Map[Numeric[_],
Ordering[_]]`), i14150 (`head.get *: tail.get` against `Out = HRes *: TRes` through `Aux`
refinements), t3199b (`java.util.Arrays.asList(Array(1,2,3):_*)`, a Java varargs — coordinate
with the javalib track's capture conversion), context-functions (`m.trans(m.a)` with `type
Ctx[T] = String ?=> Int ?=> T`: the alias with an open argument is not seen as a context function
expected type, so `m.a` is applied and "no implicit argument" follows) [V dumps].
Application: i11583 (`"hello" :*: (true, 23)`, `Y/apply_insertion.rs:64`), i4364a (`foo(f)` with
overloaded `f` against a Java SAM, `Y/eta.rs:186`), i8903 (`Y/named_args.rs:181` "lifted call of
a curried method"), tupled-function-apply (`:40`), i4600b (`Y/apply.rs:130` dependent result with
a default argument), iarrays (`xs(i)` on an `IArray`, "no applicable alternative"), UnrolledBuffer
(`new UnrolledBuffer[Int]`, "default getter" for a constructor with an implicit clause after an
empty one [I]). Inference: distinct, i7960 (`Y/infer.rs:180`), i23179 (`:175` skolem).
sticky-extmethod (wrong output: `1.m("xx")` picks the top-level `m` over the extension, research
34) is B's, not E's: the dispatch between a selection's extension candidates and an in-scope
method of the same name lives with the extension calls [V tsv, I site].

### 2.12 Namer, literals, references (10 tests, ~150 lines)

- "ambiguous reference" (`Y/find_ref.rs:138`): t4537 (`import a.*` in `package b.c` beside a
  `Settings` of package `b` in another file), triemap-hash (`import collection.*` beside
  `scala.concurrent`); dotty's `findRefRecur` (`T/Typer.scala:266-340`, `BindingPrec` `:65`): a
  wildcard import in an inner scope shadows a package member of an enclosing package from another
  file [V rule, I our site]. ~30.
- i6816 (`val foo1, foo2 = new Foo {}`, `Yn/params.rs:125`/`Yn/mod.rs:147` "definition tree
  shared by two definitions"): desugar the pattern definition with a copy of the right-hand side
  per name (`crates/syntax/src/desugar/`) [I]. ~20.
- i5823 (`def foo(a: A|Null)` beside `def foo(b: B|Null)`, "double definition"): `C | Null`
  erases to `C` (`C/TypeErasure.scala:101,340` `AndOrType`), ours to `Object`
  (`crates/tasty/src/erasure/`) [V texts, I site]. ~20.
- genericNumLits (`val x: BigInt = 13232202002020202020202`, `Y/literals.rs:45`; `T/Typer.scala:1165`
  `typedNumber` with `FromDigits`; also macros BigFloat). ~40.
- i2266 (`inline val MV = Int.MaxValue`, `Y/defs.rs:37` "inline value … or impure": the
  module selection fails `is_pure_expr`; dotty's `isPureRef` (`ast/TreeInfo.scala:708`) takes a
  stable member of a stable prefix as pure) [V site]. ~10.
- typeclass-derivation2d (`inline val size = constValue[…]` inside an inline method,
  `Yn/inline_checks.rs:32`): dotty's restriction covers nested inline *methods* only [I]. ~5.
- Parser (`export ops.*` inside a top-level `extension` block, `Yn/export_paths.rs:21`
  "top-level export": the owner is the package object), targetName-separate ("constructor proxy"
  of a class of an earlier group, `Y/constructor_proxies.rs:35`), i6716 (`Yn/checks.rs:54`
  "top-level opaque type"; `T/Namer.scala:1178`; also macros i18283, tasty-dealiasKeepOpaques).
  ~40.

### 2.13 Erased definitions and `saferExceptions` (21 tests, ~300 lines)

Research 37 §2.17 unchanged [V texts]: parser `erased` in parameter clauses (erased-7, -18, -19,
-20, -21, -25, -26, -poly-ref, -lambdas, i11996, i25725), `Yn/params.rs:119` "erased parameter"
(erased-machine-state; quotes-add-erased, quotes-reflection, which are macro tests too; macros
i17105b, term-show), `Y/function_types.rs:143,168` (safeThrowsStrawman, -2; macros
reflect-isFunctionType), `Y/constructors.rs:57` "new of an erased class" (i13691, i23305),
`throws` as `CanThrow[E] ?=> T` ("wrong number of type arguments": i13691b, i13961a,
saferExceptions). i16943 (`caps.unsafe.unsafeErasedValue`) is out.

### 2.14 Macros (13 tests of `tests/run`, 125 stops of `run-macros`)

By tests per line in `run-macros` [V counts §1.2]: the reflect slices W1–W6 of research 26
(`SuperTypeTest` alone 12 tests, the `*TypeTest` extractors 17; `Symbol.tree` of classpath
symbols 4; `Symbol.newClass` with traits and type parameters 3; the `toString`/`show` printers 14
through `M/show_code/`), then `typeChecks` (research 26 C: 9 tests of `tests/run`, 1 crash in
`run-macros`, reflect-inline), splices other than macro calls and hole patterns (8, research 26 P
and B), macro annotations in quotes and library inline bodies (3), the interpolation trees
(tasty-interpolation-1, xml-interpolation-2) and the transparent extractors i8530, i8577i. The
`tests/run-macros` rows in §1.2's last line that belong to §2's groups (adaptation 7, erased 4,
library inline bodies 4, …) go with those batches, not G.

## 3. The issues in scope [V tsv; the coordinator's verification of the TODO issues, 2026-10-26]

Done on main, to close: #60 (i9939), #89, #90 (the forward-reference check), #139 (hmap,
named-tuple-ops, tuple-product, i13968), #150 (i1915, i2004, i2004b), #152 (i1856, i4451), #159,
#160, #162 (Signals2, t2754, i8662, liftedTry, t3502, t7200), #161 (i209; typeclass-derivation2d
is another refusal, §2.12). #88's tests pass (genericValueClass, i15725, t13033, t5866, t6260,
vc-equals, mixin-primitive-on-generic-4/-5, lambda-sam-bridge, i24573); its rest, local lazy
values of value classes, is the lowering track's. #151's item 3 (TupleXXL lowering) is open now
that the typer types XXL tuples.

Open, folded into the batches (each item names its owner; §4 repeats them per batch):

| issue | left | batch |
|---|---|---|
| sticky-extmethod | **wrong output** through our frontend: `1.m("xx")` takes the top-level `m` over the extension (research 34), a silent miscompile | B, first |
| #166 | i9530, i11583 (§2.11); t4579 `java.lang.String` for `Predef.String` in inferred override results (with #105) | E |
| #166 | manifest-summoning, iterator-from, i3539 (§2.7) | B |
| #164 | polymorphic givens as import bounds (research 33's rest) | B |
| #164, #127 | pattern-bound type variables: i5077, i15302a, typelevel-peano (§2.8) | C |
| #127 | i13433b (§2.2) | B; fully-abstract-nat, -4, -6, -7 stay (§1.1) |
| #156 | `NamedTuple` prefix printing (named-tuples1, i7247, i22192, i25240, fieldsOf, i7034), i576, i7477 | A |
| #155 | extmethods2 (§2.6) | B |
| #155 | typeclass-derivation2a (§2.5) | A |
| #155 | fully-abstract-interface (§2.11) | E |
| #154 | productElementName (§2.3) | E |
| #146 | `fully_defined` forcing in `Y/named_tuples.rs:157`; namedtuples-buildcolumn (§2.10) | A |
| #129 | i14540-priorRun, library sealed classes (§2.9) | C |
| #129 | typed trees of class-defining inline arguments differ (`Yi/copy.rs`) | lowering track |
| #122 | refined type trees printed as `TypeTree`: i4031, i12677, i15525, i3058 (§2.5) | A |
| #122 | skolem prefixes i583a, i6199b (`Y/deskolemize.rs`) | E |
| #120 | `One + One` compile-time ops; the recursive alias `Last[(Int, String)]` "match type that may reduce"; i10511, i10930 dependent matches (§2.8) | C |
| #112, #105 | `new C().f("a")` typed `C.T` instead of `String`; `java.lang.String` for `Predef.String` in inferred arguments (`updated(x, x.toString)`) | E |
| #124 | accepted wrong overrides: `lazy val` over `val`, `def` over a `val` parameter, override of a final `def` (`Y/overrides.rs`) | E |
| #123 | implicit candidates not interpolated (`TC[Int]` for dotty's `TC[AnyVal]`) | B |
| #147 | byname-implicits-9, -17, generic (`Y/implicits/measures.rs`) | B |
| #137 | `Y/classes.rs:47` class type parameters completed before parents | A |
| #158, #125, #74 | pos/i2741 adaptation; i3129's LOCAL flag (`Y/access.rs`); the extra scope of a poly application as a `while` body | E, last |
| #41 | GADT (gadts, i14983, i16785, enum-Tree) | no batch |
| #108, #99, #88 rest | lowering NIR differences, local lazy values of value classes | lowering track |

The coordinator's own measurement of main (1320 pass, tuple-product as a frontend timeout) differs
from this one (1321) by that timeout only: `SCIVOLO_TIMEOUT` flakes under load [V tsv].

## 4. Batches with disjoint files and features

| batch | branch | § | tests | lines | owns |
|---|---|---|---:|---:|---|
| A types and kinds | `typer-types-2` | 2.4, 2.5, 2.10, #156, #137 | 24 | ~300 | `crates/typer/src/tpd/print/`, `Y/classes.rs`, `Y/class_bound.rs`, `Y/avoid*.rs`, `Y/stats.rs`, `Y/refined_types.rs`, `Y/refinement_members.rs`, `Y/structural.rs`, `Y/dynamic.rs`, `Y/type_trees.rs` (except the match-type arms C needs: `:82`, `:134`), `Y/type_lambdas.rs`, `Y/type_args.rs`, `Y/type_apply.rs`, `Y/type_arity.rs`, `Y/named_type_args.rs`, `Y/closures.rs`, `Y/dependent_function*.rs`, `Y/parents.rs`, `Y/parent_trees.rs`, `Y/constructors.rs:37,98`, `Y/select.rs:136`, `Y/call_types.rs`, `Y/named_tuple*.rs` |
| B implicits | `implicits-2` | 2.1, 2.2, 2.6, 2.7, sticky-extmethod | 31 | ~400 | `Y/implicits/` except C's mirror files, `Y/implicit_views.rs`, `Y/extension_calls.rs`, `Y/extension_fallback.rs`, `Y/member_views.rs`, `Y/context_bounds.rs`, `Y/import_selectors.rs`, `Y/imports.rs`, the extension branch of `Y/select.rs` (sticky-extmethod) |
| C match types, patterns, mirrors | `match-types-patterns-2` | 2.8, 2.9 | 15 | ~300 | `Y/matches.rs`, `Y/match_type_trees.rs`, `Y/patterns.rs`, `Y/unapply*.rs`, `Y/constrain_pattern.rs`, `Y/irrefutable.rs`, `Y/deriving*.rs`, `Y/derived.rs`, `Y/implicits/{sum_children,sum_mirrors,product_mirrors,mirror_types,mirror_sources,anon_mirrors,tuple_mirrors,child_prefixes}.rs`, `crates/tasty/src/subtype/match_types/`, `crates/typer/src/sealed.rs`, the two arms `Y/type_trees.rs:82,134` |
| E adaptation, application, namer | `adaptation-namer-2` | 2.3, 2.11, 2.12, #112, #105, #124, #125 | 34 | ~400 | `Y/access.rs`, `Y/adapt.rs`, `Y/adapt_by_result.rs` (B reads it, E edits it), `Y/apply*.rs`, `Y/applicability.rs`, `Y/apply_insertion.rs`, `Y/eta.rs`, `Y/named_args.rs`, `Y/default_args.rs`, `Y/infer.rs`, `Y/deskolemize.rs`, `Y/context_functions.rs`, `Y/overrides.rs`, `Y/inherited_conflicts.rs`, `Y/name_clashes.rs`, `Y/find_ref.rs`, `Y/qualifies.rs`, `Y/idents.rs`, `Y/literals.rs`, `Y/defs.rs`, `Y/constructor_proxies.rs`, `Yn/` except `Yn/params.rs:119` (F), `crates/syntax/src/desugar/`, `crates/tasty/src/erasure/` |
| F erased definitions | `erased-definitions-2` | 2.13 | 21 | ~300 | `crates/syntax/src/parse/`, `Yn/params.rs` (the erased arm), `Y/function_types.rs`, `Y/constructors.rs:57`, `L/erasure/` (read-only unless needed: the lowering track owns `L/`, tell it) |
| G macros | `macros-2` | 2.14, §1.2 | 13 + 125 | — | `crates/macros/`, `overlay/`, `Yi/reflect*.rs`, `Yi/quote*.rs`, `Yi/quote_matcher/`, `Yi/splice.rs`, `Yi/macro_*.rs`, `Yi/code_of.rs`, `Yi/compiletime.rs` (for `typeChecks`), `Yu/quotes.rs`, `Yu/quote_patterns.rs`, `Yu/splices.rs`, `Y/inline_refs.rs`, `Y/transparent.rs`, `Y/unapply.rs:65` (one arm, agreed with C), `scripts/dotty_sources.py` (`-Xmacro-settings`) |

Feature boundaries (each feature has exactly one owner; a batch whose test needs another's feature
notes it and moves on):

- A: term refinements of local class bounds; refinement type trees of every shape and their
  printing (#122 items i4031, i12677, i15525, i3058; #155's typeclass-derivation2a); structural
  calls on any `Selectable`, with implicit clauses; closure parameter types from wildcards and
  open context-function parameters; higher-kinded aliases and parent type-argument inference; the
  arity check of F-bounded higher-kinded parameters; polymorphic function values with
  higher-kinded parameters; `new` of a parameterized alias or of a class with prefix-dependent
  bounds; projections of aliases; class type parameters completed before parents (#137); named
  tuples: #146 (`fully_defined` forcing, namedtuples-buildcolumn) and #156 (`NamedTuple` prefix
  printing: named-tuples1, i7247, i22192, i25240, fieldsOf, i7034; i576, i7477).
- B: first sticky-extmethod (wrong output: the extension `m` must win over the top-level `m`);
  the implicit scope (companions through prefixes and paths, base classes, refinements);
  candidates that are conditional givens or refined values (#155's extmethods2); `ClassTag`
  (#127's i13433b), `Manifest` (#166), `TupledFunction` synthesis; search shapes (higher-kinded
  variables, unions, nested defaults, by-name dictionaries, ambiguity ranking of conversions
  (#166's i3539) and extension containers, depth (#166's iterator-from)); divergence measures
  outside the subset (#147: byname-implicits-9, -17, generic); interpolation of implicit
  candidates (#123); polymorphic givens as import bounds (#164). B does not synthesize mirrors
  (C) and does not reduce match types (C).
- C: dependent matches against match-type results (#120's i10511, i10930); compile-time ops on
  aliases (`One + One`) and recursive aliases that "may reduce" (#120); pattern-bound type
  variables and type tests in patterns (#164 §2.2, #127: i5077, i15302a, typelevel-peano);
  match-type aliases applied to wildcards; infix type applications in match-type bodies;
  match-type reduction in `crates/tasty`; mirror synthesis (library sealed classes: #129's
  i14540-priorRun; `@unroll`, objects, higher-kinded mirrors); the extractor-name-to-companion
  fallback.
- E: everything that ends in "adaptation", "application of a non-method", "no applicable
  alternative", "default getter", eta expansion, lifted curried calls, inference escapes (#166's
  i9530, i11583; #155's fully-abstract-interface; #158's pos/i2741); skolem prefixes (#122's
  i583a, i6199b); result types after applications on unstable prefixes (#112) and
  `Predef.String` kept in inferred types (#105, #166's t4579); overrides and inherited conflicts
  (#154's productElementName; #124's accepted wrong overrides); the LOCAL flag through other
  prefixes (#125); the `while`-body scope (#74, last); reference precedence; pattern definitions
  of several names; union erasure with `Null`; generic number literals; inline value purity;
  nested inline vals; exports in extension blocks and constructor proxies across groups;
  top-level opaque types.
- F: `erased` in parameter clauses (parser), erased parameters, erased classes, impure function
  types, `throws`.
- G: everything that names a `quotes.reflect` member, `Expr`/`Type` printing, splices and quote
  patterns, macro annotations, `typeChecks`/`typeCheckErrors`, transparent inline extractors,
  interpolation trees seen by macros, `-Xmacro-settings` in the test script.

Shared hot files [V list]: `Y/adapt.rs` and `Y/apply.rs` are E's; the running stdlib-javalib
track edits both around `Y/apply.rs:115` "result type of a wildcard argument" and `adapt.rs:124`
(capture conversion), so E merges `main` as soon as that MR lands and does not touch those arms;
`Y/implicits*` is B's except the mirror files (C); all of `crates/typer/src/inlining/` is the
lowering track's except G's macro files named above — no batch here edits `Yi/host.rs`,
`Yi/reduce/`, `Yi/inline_call.rs`, `Yi/beta.rs`, `Yi/proxies.rs`, `Yi/opaque_proxies.rs`;
`Y/type_trees.rs` is A's with C's two arms; `Y/unapply.rs` is C's with G's one arm;
`Y/constructors.rs` is A's with F's one arm; `Yn/params.rs` is E's with F's one arm.

Taken (running tracks, excluded above): stdlib-g4 (#168 G4: `runtime.Intrinsics`, `CFuncPtr`,
`c"…"`, `LinktimeInfo`/`@resolvedAtLinktime`, `@struct`; `crates/lower/src/codegen/` intrinsics
files), stdlib-javalib (#168 G5 rest, G6c wildcard capture conversion in `Y/adapt.rs`/`Y/apply.rs`,
G6d javalib singles, G7 NIR byte diffs; enums-thunk, t2849, t7336, and t3199b's Java varargs
adaptation if it falls out of G6c), lowering #165 (local objects and `super` in inline bodies,
path-dependent aliases seen from the path, local lazy values of value classes; all of `L/` and
the non-macro `Yi/` files; the 19 tests of §1.1), tooling (#91, #149, #153, #128).

Order within a batch by tests per line (§2's table); B and C first (their groups are the
largest), A next, E's probes as time allows, F only when a coder is free. Gates as research 27
§3: `just gates`, `typer-compare` on the probes that become golden inputs, `just
dotty-lower-report FILTER` for the batch's tests, `just snapshot-diff programs lower typer`
without worsened rows; G measures with `just dotty-lower-report "" run-macros`.
