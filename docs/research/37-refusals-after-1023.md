# 37 — Refusals in dotty's tests/run and tests/run-macros after 2026-10-23

Date: 2026-10-24. Claims are [V] verified by reading the code or running it, [I] inferred. Paths
as in research 33: `T/` = `ref/scala3/compiler/src/dotty/tools/dotc/typer/`, `C/` = `.../dotc/core/`,
`X/` = `.../dotc/transform/`, `Y/` = `crates/typer/src/typer/`, `Yn/` = `crates/typer/src/namer/`,
`Yi/` = `crates/typer/src/inlining/`, `Yu/` = `crates/typer/src/unpickle/`, `L/` =
`crates/lower/src/phases/`, `S/` = `crates/tasty/src/symbols/`. Measured on main 409ba6be with
`just dotty-lower-report` (`tmp/dotty/3.9.0-0.5.12/lower-results.tsv`): **1263 pass** of 1711,
222 typer refusals in 108 distinct texts, 5 lowering, 14 parser, 1 frontend timeout
(genericTupleMembers), 1 wrong output (sticky-extmethod), 32 fail in the VM also under Scala Native,
173 skipped [V]. `just dotty-lower-report "" run-macros` (`lower-results-run-macros.tsv`): 321 tests,
**134 pass**, 145 typer refusals in 69 texts, 21 crash in the VM, 9 wrong output, 2 frontend crashes,
1 lowering, 1 earlier group, 8 skipped [V]. Method: texts grouped by their site (grep of the text);
the refusing source line of each of the 206 single-file tests of `tests/run` from
`scivolo-typer-dump` (`tmp/r37-run-lines.txt`), the 16 directory tests attributed by text; probes
under `tmp/r37/` [V].

**Recommendation.** 64 of the 243 frontend stops in `tests/run` belong to the parallel wave or to
closed tracks (§1); `tests/run-macros` is almost entirely the macro track's (§1.2). Of the rest,
six batches with disjoint files (§4): A types (block types of `Selectable` anonymous classes,
refinements, dependent function types; 14 tests, ~220 lines), B namer, stubs and value classes
(`@static` bug, runtime classes without TASTy, i15725, opaque types, exports, library inline
bodies; 25 tests, ~330), C mirrors and match types (#129, #120, mirror patterns in the inliner,
`codeOf`; 30 tests, ~420), D named tuples and kinds (#139, #146, higher-kinded arguments, XXL
tuples, named type arguments; 25 tests, ~370), E implicits and adaptation probes (#166 and ~35
singles, research 28's method), F erased definitions (21 tests, ~300, experimental, last). The
typer issues #159, #160, #162 are done and #154, #155, #161 are down to one leftover each (§3).

## 1. Attribution

### 1.1 tests/run: 243 frontend stops

| where | tests | names |
|---|---:|---|
| current wave: research 33 §2.7 Java enums | 4 | enum-constrs, enum-custom-toString, enum-ordinal-java, enum-values-order (`Y/classes.rs:105`) |
| current wave: research 33 §2.2, §2.6 | 2 | i5077 (`Y/unapply.rs:255` "pattern-bound type against a wildcard argument"), CollectionTests (`Y/overrides.rs:136`) |
| current wave: research 34/35 inliner | 12 | "implicit search without the call's context" (`Yi/host.rs:39`): erased-inline-product-3, i11174, i11174local, i11542, i12052, i12328, i15988b, i8530-b, sip23-valueof; i11050 (`:41`); transparentPrivates (I5); i23279 |
| current wave: research 34 lowering | 4 | mixins1 (L5), implicitFuns2 (L3), i11961 (`L/lift_names.rs:162`), implicit-alias (`L/local_lazy.rs:91` "local lazy value of a value class") |
| current wave: research 26 macros | 14 | quoted-sematics-1, quotes-add-erased, quotes-reflection, i8530, i8577i; `typeChecks`/`typeCheckErrors` (#130, research 26 C): i11630, i12656, i18150, i22968, i7487, i7488, scala-tests-typeChecks, scala2-library-test, typeCheckErrors |
| #127 type tests | 5 | fully-abstract-nat, -4, -5, -6, -7 |
| #41 GADT | 4 | gadts, i14983, i16785, i10511 |
| #147 divergence measure | 4 | byname-implicits-17, generic, i11542a, i11563 |
| out: XML, `into`, `3.3`, spreads, capture checking, modularity | 12 | i16458, i16459, xml; convertible; drop-apply-optimization; i11419, spreads-subtyping, spread-patterns; errorhandling; i15840, i3920, given-disambiguation |
| library / VM | 3 | t2849, t7336 (javalib 0.5.12 has no `Runtime.totalMemory`: `tmp/r37/Rt5.scala` refuses it with and without `()` [V]), genericTupleMembers (frontend timeout) |

The other 179 split into §2 (≈ 130) and ≈ 50 singles, of which §2.18 lists those with a site.
Research 33's groups §2.1, §2.3, §2.5 and §3's import bounds, by-name method values and
parent arguments from `pt` no longer appear [V].

### 1.2 tests/run-macros: 187 stops

103 of the 145 typer refusals name a macro API gap, all in research 26's plan [V]: a reflect
method the overlay lacks (49: `SuperTypeTest` 10, `SymbolMethods` 8, `MethodTypeTypeTest` 5,
`Lambda`, `Assign`, `CompilationInfo`, `MethodType`, `TypeIdent`, … — W1–W6), `Symbol.newClass`
(8), a splice other than a macro call (7, `Yi/splice.rs:92`), `Symbol.requiredClass` (5),
`Symbol.tree` (4), `Printer.TypeReprStructure` (3), `Expr`/`Type`/`Symbol.toString` (5),
`Implicits.searchIgnoring`, `Select.copy`, `TypeRepr.show` (2 each), quote-pattern shapes (4),
singles (`ConstantType.apply`, `Flags.Deferred`, `TreeAnsiCode`, `defn.AnyClass`, …). The 21 VM
crashes and 9 wrong outputs are 26 `annot-*` tests plus i19676, i20449, i18806,
gestalt-optional-staging: macro annotations, research 26 [V names]. The other 42 typer refusals
share §2's groups and are counted there: adaptation 6, "type argument of another kind" 5,
`codeOf` 3, member not found 3, library inline bodies 7, erased 2, type tree ValDef 2, singles.

## 2. Groups, ranked by tests per line

| § | group | tests | lines | scivolo | dotty |
|---|---|---:|---:|---|---|
| 2.1 | `@static` literal check bug | 3 | ~10 | `Yn/check_static.rs:44-46` | `X/CheckStatic.scala` |
| 2.3 | `codeOf` through `TreeCode` | 5 | ~30 | `Yi/compiletime.rs:161` | `N/Inlines.scala` `codeOf` |
| 2.8 | mirror type patterns in the inliner | 5 | ~40 | `Yi/reduce/mod.rs:105`, `reduce/patterns.rs` | `N/InlineReducer.scala` `reducePattern` |
| 2.2 | `Selectable` anonymous classes in block types (#122) | 4 | ~40 | `Y/class_bound.rs:32-34`, `Y/stats.rs:154` | `C/TypeOps.scala:422-459` |
| 2.12 | higher-kinded type arguments | 14 | ~150 | `Y/type_apply.rs:122,133`, `Y/type_args.rs:71-73`, `Y/type_lambdas.rs:78` | `T/Applications.scala:1592` `typedTypeApply`, `C/TypeApplications.scala:71,387` |
| 2.15 | implicits tail (probe first) | 24 | ~300 | `Y/implicits/` | `T/Implicits.scala` |
| 2.6 | refinements | 6 | ~80 | `Y/refined_types.rs:14,48,62,66` | `T/Typer.scala:2704` `typedRefinedTypeTree` |
| 2.13 | library inline bodies | 9 | ~120 | `Yu/locals.rs:87`, `Yu/selections.rs:111`, `Yu/paths.rs:114`, `Yu/assign.rs:129` | `C/tasty/TreeUnpickler.scala` |
| 2.14 | opaque types outside objects, exports, XXL tuples, named type arguments | 12 | ~240 | `Yn/checks.rs:52,54`, `Yn/exports.rs:141`, `Yn/export_paths.rs:21`, `Y/select.rs:29`, `Y/pattern_args.rs:109`, `Y/type_trees.rs:130` | `T/Namer.scala`, `T/Applications.scala:1581` |
| 2.17 | erased definitions and `saferExceptions` | 21 | ~300 | `crates/syntax/src/parse/`, `Yn/params.rs:119`, `Y/function_types.rs:143,165`, `Y/constructors.rs:57` | `T/Typer.scala:1795-1828`, `X/Erasure.scala` |
| 2.9 | mirrors (#129) | 9 | ~150 | `Y/implicits/sum_children.rs:18,57`, `sum_mirrors.rs:29`, `product_mirrors.rs:66`, `mirror_types.rs:102`, `L/mirrors.rs:57` | `T/Synthesizer.scala` |
| 2.10 | match types (#120) | 11 | ~200 | `Y/type_trees.rs:53,80,85`, `Yn/complete.rs:19`, `crates/tasty/src/subtype/match_types/` | `C/MatchTypeTrace`, `C/TypeComparer.scala` `matchCases` |
| 2.4 | `scala.runtime` classes without TASTy | 3 | ~60 | `S/java_stubs.rs` | — |
| 2.7 | dependent function types | 4 | ~100 | `Y/type_trees.rs:130`, `Y/closures.rs:133` | `T/Typer.scala:1795-1828` `typedDependent` |
| 2.11 | named tuples (#139, #146) | 5 | ~120 | `Y/named_tuples.rs`, `Y/call_types.rs:28`, `Y/pattern_args.rs:109` | `T/Typer.scala` `tryNamedTupleSelection` |
| 2.5 | retained inline method in a value class (#88) | 1 | ~30 | `Y/retained.rs:74`, `L/retained_bodies.rs`, `L/extension_methods.rs` | `X/Erasure.scala:976` |
| 2.16 | adaptation singles (probe first) | 13 | — | `Y/adapt.rs:129` | — |

### 2.1 `@static` values initialized by a literal (3 tests, ~10 lines)

i5924b, i5924c and statics stop at `@static val x = 10` with "@static value initialized by more
than a literal" [V dumps]; the probe `tmp/r37/Static.scala` (`@scala.annotation.static val x =
10`) refuses the same way [V]. `Yn/check_static.rs:40-46` looks the symbol's tree up through
`by_tree.iter().find(…)` and tests its `rhs` for `Tree::Literal`; the tree found is not the
`ValDef` with the literal [I: the companion's forwarder or the getter]. Fix: take the `ValDef` of
the symbol itself (`syms.def_trees` as `sum_children.rs:17` does).

### 2.2 `Selectable` anonymous classes in block types (4 tests, ~40 lines)

i13703 (`new Foo { var i: Int = 0 }`, `Foo extends reflect.Selectable`), i14340 (`new Container1
{ def foo = Foo(10) }`), i24056 (`new scala.reflect.Selectable: val o: DFValOf[DFBit] = …`),
selectable-new (`class C extends reflect.Selectable` in a block) [V dumps]. `Y/class_bound.rs:32-34`
returns `None` for a term member that needs a refinement, so `no_local_refs` (`Y/avoid.rs:70`)
fails and `Y/stats.rs:154` refuses [V]. dotty's `classBound` (`C/TypeOps.scala:422-459`) refines
the parents with every non-private member whose info narrows the inherited one or, in a
`Selectable`, that overrides none (`:443-446`): `RefinedType(parent, decl.name, avoid(decl.info,
missingDecls))` for term members too [V]. Port: in `local_class_bound`, build `Type::Refined(bound,
name, info)` with the member's info avoided like the type members (`:35-42`); `Y/refinement_members.rs`
already types structural term refinements [V]. The selection `f.i` then goes through
`Y/structural.rs` (reflect.Selectable's `selectDynamic`) [I]. #122 item 1 (`RefinedTypeTree`
printing) is adjacent, same files.

### 2.3 `codeOf` (5 tests, ~30 lines)

`Yi/compiletime.rs:161` accepts Int, Long and Boolean literals only: i13181 (`codeOf(1+2)`), i13183
(`codeOf("\u001b")`) in `tests/run`; i6622, i8306, inline-beta-reduce-polyfunction in `run-macros`
[V]. Research 26 S's `Expr.show`/`TreeCode` printer is in (ROADMAP item 2, 2026-10-23): print the
argument tree with it, as dotty's `codeOf` (`N/Inlines.scala:490`) prints the tree [I].

### 2.4 `scala.runtime` classes without TASTy (3 tests, ~60 lines)

t13033 (`scala.runtime.Statics.mix`), t8601c (`scala.runtime.IntRef`), i4205 (`import
scala.runtime.BoxedUnit`) stop at "member not found" / "import of a missing member" [V dumps];
the probes `tmp/r37/Runtime2.scala`, `Runtime4.scala`, `Boxed.scala` refuse at exactly those names
[V]. These are Java classes of scala-library (`scala-library-3.9.0.jar` has only their `.class`,
`scalalib_native` only `.nir`) [V `unzip -l`]. `S/java_stubs.rs` enters one such class by hand
(`scala.math.ScalaNumber`, 24 lines) [V]. Port: stubs for `Statics` (`mix`, `finalizeHash`,
`anyHash`, `ioobe`, `releaseFence`), the seven `*Ref` classes (`elem`, `create`, `zero`) and
`BoxedUnit.UNIT`, ~20 lines each, in files under `S/` by class; or decode the classfiles'
signatures (research 08 §3 option A, larger). t13033 is #88's second test; i15725 (§2.5) the first.

### 2.5 Retained inline method in a value class (1 test, ~30 lines)

i15725: `class ToString(d: Int) extends AnyVal: inline override def toString(): String` stops at
`Y/retained.rs:74` [V]. dotty keeps `toString$retainedBody` in the class, `Erasure.addRetainedInlineBodies`
(`X/Erasure.scala:976-1038`) moves the body back, and `ExtensionMethods` moves `toString` to
`toString$extension` like any method [V]. Ours: `L/retained_bodies.rs` runs before
`L/extension_methods.rs` (`L/pipeline.rs:11,31`) and substitutes parameters by symbol, so lifting
the guard should work once `move_body` keeps the receiver `this` mapped by the extension phase
[I]. Probe: `just lower-run` on i15725, compare `just nir-cmp` with Scala Native's `ToString$`.

### 2.6 Refinements (6 tests, ~80 lines)

Unchanged from research 33 §3 [V sites]: "refines no type member" (`Y/refined_types.rs:14,66`:
i22649 `TypeReprStructure` host test and implicit-functors2 `ErasedInstances { type FT = F[T]; type
C[X[_]] = F[X] }`), "refers to the refined type's members" (`:62`: i23409 `Ops[F, A] { type T =
Func[F] }`, typeclass-derivation2a `Generic[Lst[T]] { type Shape = ShapeOf[T] }`), "structural
refinement of an existing member" (`:48`: i8073, i8073b `CC { val a: aa.type }`). dotty's
`typedRefinedTypeTree` (`T/Typer.scala:2704`) types the refinements in a fresh refined-class
scope and accepts any member a parent declares as abstract or that the refinement narrows
(`checkRefinementNonCyclic`, `RefinedType` per refinement) [V]. Port: drop the "refines no type
member" guard for a member the parent declares (type or term), type the right-hand sides with the
refined class's `this` in scope so references between refinements resolve, and allow a value
refinement of an existing `val` with a narrower singleton type.

### 2.7 Dependent function types (4 tests, ~100 lines)

eff-dependent (`(f: Fun[A, B]) => List[A] => f.Eff ?=> List[B]`), ift-return (`A ?=> (c: Ctx) ?=>
(Int, c.T)`), i19629 and t1939 (`Y/closures.rs:133`) [V dumps]. The parser gives the parameter
list as `ValDef` trees in type position, which `Y/type_trees.rs:130` refuses as "type tree
ValDef" [V]. dotty's `typedDependent` (`T/Typer.scala:1795-1828`): an `apply` method typed in a
fresh refined-class owner, the result `RefinedTypeTree(FunctionN[params, nonDependentResult],
List(appDef))` [V]. Port: in `type_trees.rs`, a `Function { params: ValDef… }` arm building the
method type and the `Type::Refined(FunctionN[…], "apply", method)` (the method type with a binder
for the dependency); `closures.rs:133` then accepts a closure against such a refined function
type [I]. hmap's `polymorphic-functions` are separate (§2.14 named type arguments).

### 2.8 Mirror type patterns in the inliner (5 tests, ~40 lines)

i13146, i13146a, i9011, typeclass-derivation-doc-example, typeclass-derivation2b: research 33
§2.1's tests, now typed, stop in the inliner with "inline match that does not reduce"
(`Yi/reduce/mod.rs:105`) [V tsv]. The pattern `case s: Mirror.SumOf[T]` is a refined type
(`Mirror.Sum { type MirroredType = T; … }`); the scrutinee is the summoned anonymous mirror whose
type carries the same refinements with the concrete types [V sources]. dotty's `reducePattern`
(`N/InlineReducer.scala:192,288`) decides `Typed` patterns with `scrut <:< tpt.tpe`; the mirror's
`MirroredType` is a type alias member, so the refinement check is alias equality [V]. Probe first: `just typer-dump --inlined` of i9011 and `is_subtype` of the two
refined types in `crates/tasty/src/subtype/refined.rs` (likely a `MirroredElemTypes <: Tuple` bound
versus an alias) [I]. Owner: C (§4), the only batch in `Yi/reduce/`.

### 2.9 Mirrors (#129; 9 tests, ~150 lines)

i13332, i13332a, i13332anonprod ("child visible through a prefix only", `Y/implicits/sum_children.rs:57`),
i13332shapeless-b ("class selected from a path", `sum_mirrors.rs:29`), i14540-priorRun
("library's sealed class", `sum_children.rs:18`: children are not decoded from TASTy, #120 item
3 shares it), curried-mirror (lowering `L/mirrors.rs:57` "companions of generic sealed classes",
#129 item 2), unroll-multiple (`product_mirrors.rs:66` `@unroll`), deriving (`mirror_types.rs:102`
labels), deriving-from-product-typed (`fromProductTyped`, member not found) [V dumps and tsv].
dotty's `Synthesizer` (`T/Synthesizer.scala:538` `sumMirror`, `:447` `productMirror`) builds the mirror from
`cls.children` through any prefix (`TypeOps.refineUsingParent`, `C/TypeOps.scala:761`) [V].

### 2.10 Match types (#120; 11 tests, ~200 lines)

"compile-time operation type" (`Y/type_trees.rs:53`: enum-nat, i7868, typelevel-peano; macros
type-print), "match type that may reduce" (`:85`: anon-mirror-gen-local, i12328a, i12328b; macros
tasty-tree-map), "parameterized type alias applied to a wildcard" (`:80`: 9416 `type Extract[X] =
X match { case DomainImpl[t] => t }`, erased-inline-product-2, t13418 `case _: Foo[_]`), recursive
match-type aliases as "cyclic reference" (`Yn/complete.rs:19`: i10930, Providers; #120 item 5)
[V dumps]. The issue lists the eight items with dotty's sites.

### 2.11 Named tuples (#139, #146; 5 tests, ~120 lines)

i22150 (`tup.x`, member not found), named-tuple-ops (`Y/inline_refs.rs:43` compiletime op in
`NamedTuple` ops), named-tuples-strawman-2 ("type tree Apply": `N *: IndicesWhereHelper[t, P,
S[N]]` in a match type), namedtuples-buildcolumn (`Y/call_types.rs:28` "type argument of a
transparent call bounded from both sides"), named-tuples-xxl (`Y/pattern_args.rs:109`) [V dumps].
#146 names dotty's `tryNamedTupleSelection` (`T/Typer.scala:860`).

### 2.12 Higher-kinded type arguments (14 tests, ~150 lines)

"type argument of another kind" (`Y/type_apply.rs:133`, `Y/type_args.rs:73`): i13146poly,
poly-kinded-derives (`Mirror { type MirroredType[X] = F[X]; type MirroredElemTypes[_] }`),
macros i16734a, -b, -c (`K1Inv[F[A]]`, variance-annotated HK parameters), type-members, type-show
(`show[x.type]`); "type application of a non-polymorphic method" (`type_apply.rs:122`: i13304
`test[List][Option]` on a polymorphic function value, interleaving `lookupOrElse[Option[Int]]`
with clause interleaving); "higher-kinded type alias overriding a type member" (`type_lambdas.rs:78`:
decorators, i22888 `type A[T] = Int`); tyql ("type constructor without arguments",
`type_trees.rs:140`); t2594_tcpoly (`Bar1[X]#And`, `type_args.rs:147`); macros tasty-simplified
(wildcard HK argument, `type_args.rs:71`) [V dumps and tsv]. dotty: `typedTypeApply` applies a
`PolyFunction`'s `apply`; `C/TypeApplications.scala:71` `EtaExpansion` and `:387` `etaExpandIfHK`
turn a type constructor argument into a type lambda of the parameter's arity and variance
[V sites, I rule].

### 2.13 Library inline bodies (9 tests, ~120 lines)

`Yu/paths.rs:114` "reference to a class parameter in a library inline body" (gestalt-optional-inline,
publicInBinary), and in `run-macros` `Yu/locals.rs:87` "closure of a method without parameters"
(quote-match-poly-function-1, -1-regression, -2), `Yu/selections.rs:111` "selection of an unknown
member" (i15968, i6253-b, quote-elide-prefix), `Yu/assign.rs:129` "constructor call" (quote-implicitMatch)
[V tsv]. Each is a `TreeUnpickler` shape not yet mirrored; the files are research 35 S4's, done
[V]. `Yu/quotes.rs` and `Yu/quote_patterns.rs` stay with the macro track.

### 2.14 Opaque types, exports, XXL tuples, named type arguments (12 tests, ~240 lines)

- Opaque types outside objects (`Yn/checks.rs:52,54`): i12914, option-extract, i6716; macros
  i18283, tasty-dealiasKeepOpaques (5). dotty puts top-level opaque types in the package object
  and refines classes' `this` with the alias (`T/Namer.scala:1178`, `C/SymDenotations.scala:444`
  `opaqueToBounds`) [V]. ~80.
- Exports: Parser (`Yn/export_paths.rs:21` top-level `export ops.*`), i10724, i14020 (`Yn/exports.rs:141`
  inline or erased member), macros exports (`Yn/export_members.rs:36`) (4). ~60.
- Generic tuples over 22 elements (`Y/select.rs:29`): i13968, tuple-product; named-tuples-xxl
  (§2.11) (3). ~60, `Y/tuple_xxl.rs`.
- Named type arguments (`language.experimental.namedTypeArguments`): hmap, hmap-covariant
  (`HEntry[K = "name"]`), polymorphic-functions (`t11[F = C11]`) — "type tree NamedArg"
  (`Y/type_trees.rs:130`); dotty `typedNamedArgs` (`T/Applications.scala:1581`) reorders them
  (3). ~40.

### 2.15 Implicits tail (24 tests, ~300 lines, probe first)

`Y/implicits/extension_views.rs:29,68,74` (extmethods2, fragables-extension, extension-specificity2,
extmethod-overload, i9928; extmethods2 is #155's leftover), `conversions.rs:83` ambiguous (i3539,
t8280), `unsettled.rs:29` HK variables (i13986, i13987), `view_kinds.rs:80` (byname-implicits-6,
i23897), `scope.rs:41,85` (typealias_overriding, spreads), "no implicit argument" (i17115,
implied-specifity-2, manifest-summoning, tupled-function-untupled), `search.rs:72,109`
(iterator-from, enrich-gentraversable), `dictionary.rs:89` (byname-implicits-28), `clauses.rs:56`
(implied-priority), `adapt_by_result.rs:53` (tagless), `class_tags.rs:43,72` (i13433b, i22345)
[V dumps]. Research 28's method: `typer-compare` probe per test before coding.

### 2.16 Adaptation singles (13 tests, probe first)

`Y/adapt.rs:129` "adaptation": context-functions (`m.trans(m.a)`), enum-Tree (`case True =>
true`), fully-abstract-interface (#155 leftover), i14150, inline-numeric (`scala.math.inline.*`),
instances-anonymous and extension-methods (`x.compareTo(y)`, #166 item 1), lst, string-context-implicits-with-conversion,
t3199b, transparentArrowAssoc, i16065 (#166 item 2), i9530 (#166 item 1) [V dumps]; also
sticky-extmethod (wrong output: `1.m("xx")` picks the top-level `m` over the extension, research
34) and i7359 (`Y/overrides.rs:136` "abstract member overriding a concrete one" on `trait
ObjectInterface`).

### 2.17 Erased definitions and `saferExceptions` (21 tests, ~300 lines)

Parser: `erased` in method and lambda parameter clauses (`def f(erased i: Int)`, `(erased x: Int)
=> …`): erased-7, -18, -19, -20, -21, -25, -26, -poly-ref, -lambdas, i11996, i25725 — `is_erased`
(`crates/syntax/src/parse/modifiers.rs:15`) exists for modifiers and function types but the
parameter clauses do not call it [V refusal texts, I site]. Typer: `Yn/params.rs:119` "erased
parameter" (erased-machine-state; macros i17105b, term-show), `Y/function_types.rs:143,165`
(safeThrowsStrawman, -2), `Y/constructors.rs:57` "new of an erased class" (i13691, i23305),
`throws` as `CanThrow[E] ?=> T` ("wrong number of type arguments": i13691b, i13961a, i15998,
saferExceptions), i16943 (`unsafeErasedValue`, a one-line guard in D's `Y/inline_refs.rs:46`)
[V dumps]. dotty: erased
parameters stay in `MethodType` with `paramErasureStatuses` and are dropped at erasure
(`X/Erasure.scala`, `C/Types.scala` `hasErasedParams`); `L/erasure/erased_defs.rs` drops erased
definitions already [V]. Experimental features with no library demand: last.

### 2.18 Singles with a known site [V dumps]

"local type escaping through a type variable" (`Y/infer.rs:180`: distinct, i7960; macros
quote-matcher-runtime), "skolem in the type of a definition" (`infer.rs:175`: i23179), "deferred
given" (`Yn/checks.rs:62`: abstract-givens), "dependent result type" (`Y/apply.rs:130`: i4600b),
"result type of a wildcard argument" (`apply.rs:115`: enums-thunk), "application of a non-method"
(`Y/apply_insertion.rs:57`: i11583), "parameter type inferred from the target of a wildcard"
(`Y/closures.rs:57`: 16405), "context closure parameter of an open type" (`closures.rs:61`:
break-opt, loops-alt), "case type with pattern variables" (`Y/matches.rs:100`: i15302a),
"transparent inline extractor" (i8530, i8577i: macros), "derives clause of an object"
(`Y/deriving.rs:22`: typeclass-derivation3), "inline definition nested in an inline method"
(`Yn/inline_checks.rs:32`: typeclass-derivation2d, #161's leftover), "conflicting inherited
members" (`Y/overrides.rs:167`: productElementName, #154's leftover), "default getter"
(`Yn/defaults.rs`: UnrolledBuffer), "generic number literal" (`Y/literals.rs:45`: genericNumLits;
macros BigFloat), "projection of a type alias" (`Y/select.rs:137`: t7985b), "package used as a
value" (`Y/access.rs:43`: pkgobjvals), "ambiguous reference" (`Y/find_ref.rs`: t4537,
triemap-hash), "unresolved name AnyRef" (`Y/idents.rs:62`: t4770 `@specialized(AnyRef)`),
"definition tree shared by two definitions" (`Yn/params.rs:125`: i6816 `val foo1, foo2 = new Foo
{}`), "inline value of a type that is not constant" (`Y/defs.rs:34`: i2266 `Int.MaxValue`),
"lifted call of a curried method" (`Y/named_args.rs:181`: i8903), "new of a parameterized type
alias" (`Y/constructors.rs:37`: i4557), "double definition" (i5823), "overloaded reference"
(returning), "method value of a SAM type" (`Y/eta.rs:181`: i4364a), "inline method of an object
with opaque types" (`Yi/opaque_proxies.rs`: i13461-b), "inline expansion of a type referring to
a binding" (`Yi/inline_call.rs:70`: opaque-inline2, tuple-map), "array passed to a repeated
parameter of an inline method" (`Yi/proxies.rs:34`: i6858), "beta reduction with a by-name
parameter" (`Yi/beta.rs:151`: i20095), "reference to an erased scrutinee" (erased-inline-product-1),
"structural call on a Selectable other than reflect.Selectable" (`Y/structural.rs:81`: i17761,
structural-contextual), "structural refinement with a context parameter"
(`Y/refinement_members.rs:85`: structural, structural-compat), "sequence wildcard before the last
pattern" (spread-patterns), "named or default arguments of a repeated method" (tupled-function-apply),
"higher-kinded type alias overriding a type member" (§2.12).

## 3. The typer issues in scope [V tsv]

| issue | state |
|---|---|
| #154 overrides through self types | i12032, i8582 pass; left: productElementName (`Y/overrides.rs:167`) → E |
| #155 apply insertion through views | t4660 passes; left: extmethods2 (`extension_views.rs:68`), fully-abstract-interface (adaptation) → E |
| #159 setters, #160 catch handlers, #162 inherited result types | Signals2, t2754, i8662, liftedTry, t3502, t7200 pass: close |
| #161 clashing synthetic members | i209 passes; left: typeclass-derivation2d (`Yn/inline_checks.rs:32`, another refusal) |
| #166 research 27/28 singles | extension-methods, i16065, i9530 (adaptation), manifest-summoning, iterator-from, i3539 (implicits) still refused → E |
| #120, #122, #129, #139, #146 | §2.10, §2.2, §2.9, §2.11 |
| #156, #157, #158 | typed-tree printing in `tests/pos`; not measured here (`just typer-report`) |
| #88 value classes | genericValueClass passes; i15725 (§2.5), t13033 (§2.4) → B |

## 4. Batches with disjoint files

| batch | branch | § | tests | lines | owns |
|---|---|---|---:|---:|---|
| A types | `typer-types-1` | 2.2, 2.6, 2.7 | 14 | ~220 | `Y/class_bound.rs`, `Y/avoid*.rs`, `Y/stats.rs`, `Y/refined_types.rs`, `Y/refinement_members.rs`, `Y/structural.rs`, `Y/type_trees.rs`, `Y/closures.rs` |
| B namer, stubs, value classes | `namer-stubs-1` | 2.1, 2.4, 2.5, 2.13, 2.14 (opaque, exports) | 25 | ~330 | `Yn/check_static.rs`, `Yn/checks.rs`, `Yn/exports.rs`, `Yn/export_paths.rs`, `Yn/export_members.rs`, `S/java_stubs.rs` (+ new `S/` files), `Y/retained.rs`, `L/retained_bodies.rs`, `L/extension_methods.rs`, `Yu/locals.rs`, `Yu/selections.rs`, `Yu/paths.rs`, `Yu/assign.rs` |
| C mirrors, match types | `mirrors-match-types-1` | 2.3, 2.8, 2.9, 2.10 | 30 | ~420 | `Y/implicits/{sum_children,sum_mirrors,product_mirrors,mirror_types,mirror_sources,anon_mirrors,tuple_mirrors}.rs`, `L/mirrors.rs`, `Yi/reduce/`, `Yi/compiletime.rs`, `Yi/type_eval.rs`, `Y/match_type_trees.rs`, `Yn/complete.rs`, `crates/tasty/src/subtype/match_types/` |
| D named tuples, kinds | `named-tuples-kinds-1` | 2.11, 2.12, 2.14 (XXL, named type arguments) | 25 | ~370 | `Y/named_tuples.rs`, `Y/named_tuple_types.rs`, `Y/pattern_args.rs`, `Y/call_types.rs`, `Y/tuple_xxl.rs`, `Y/small_tuples.rs`, `Y/select.rs`, `Y/type_apply.rs`, `Y/type_args.rs`, `Y/type_lambdas.rs`, `Y/inline_refs.rs` |
| E implicits, adaptation | `implicits-adaptation-1` | 2.15, 2.16, 2.18 (probes) | ~40 | ~400 | `Y/implicits/` minus C's files, `Y/adapt.rs`, `Y/adapt_by_result.rs`, `Y/apply*.rs`, `Y/infer.rs`, `Y/eta.rs`, `Y/overrides.rs`, `Y/matches.rs`, `Y/deriving.rs` |
| F erased definitions | `erased-definitions-1` | 2.17 | 21 | ~300 | `crates/syntax/src/parse/`, `Yn/params.rs`, `Y/function_types.rs`, `Y/constructors.rs`, `crates/tasty/src/erasure/`, `L/erasure/` |

Rules [I]: the guards C makes unnecessary in `Y/type_trees.rs:53,80,85` stay in place (A owns the
file) and stop firing; the three "type tree …" arms A adds (`:130`) must not touch D's `NamedArg`
case — D adds `NamedArg` handling in `Y/type_apply.rs` before the type tree is typed. B's `Yu/`
files are research 35's; the macro track owns `Yu/quotes.rs`, `Yu/quote_patterns.rs`,
`Yu/splices.rs` and `Yi/splice.rs`, `Yi/quote_match*`, `Yi/reflect*`. The current wave owns
`Y/unapply.rs`, `Y/classes.rs`, `Yn/base_types.rs`, `Yi/copy*.rs`, `Yi/this_proxy.rs`,
`Yi/outer_this.rs`, `Yi/host.rs`, `Y/accessor_defs.rs` and `L/` outside the files named above
— no batch here edits them. Order within a batch by tests per line (§2's table); A, B and C
first, F only when a coder is free. Gates as research 27 §3: `just gates`, `typer-compare` on the
probes that become golden inputs, `just dotty-lower-report FILTER` for the batch's tests, `just
snapshot-diff programs lower typer` without worsened rows, and for B a `tests/programs` entry
using `Statics`/`IntRef`.
