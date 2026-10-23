# 33 — Typer refusals in dotty's tests/run

Date: 2026-10-23. Claims are [V] verified by reading the code or running it, [I] inferred. Paths
as in research 27: `T/` = `ref/scala3/compiler/src/dotty/tools/dotc/typer/`, `C/` = `.../dotc/core/`,
`X/` = `.../dotc/transform/`, `D/` = `.../dotc/ast/`, `N/` = `.../dotc/inlines/`, `Y/` =
`crates/typer/src/typer/`, `Yn/` = `crates/typer/src/namer/`, `Yi/` = `crates/typer/src/inlining/`,
`Yu/` = `crates/typer/src/unpickle/`. Measured with `just dotty-lower-report` on main 342a4d47
(`tmp/dotty/3.9.0-0.5.12/lower-results.tsv`): **1195 pass** of 1711, 278 typer refusals with 118
distinct texts, 15 lowering refusals, 14 parser refusals [V]. Method: the refusal texts grouped by
their site (`just grep` of the text), the refusing line of every test of the groups below read from
`just typer-dump` [V]; the long tail is attributed by text only [I].

**Recommendation.** 80 of the 278 belong to parallel tracks or are out of scope (§1). Of the rest,
three cheap fixes unblock 21 tests for about 60 lines in three disjoint files: §2.1 Mirror type
patterns in inline matches (12 tests, `Y/type_tests.rs`), §2.3 annotated type trees in library
inline bodies (5, `Yu/tpts.rs`), §2.5 `import language.future` (4, `Y/mod.rs`). Next by tests per
line: the inliner's `this` proxies (§2.4, 8 tests), pattern-bound type variables (§2.2, 7), local
classes in inline bodies (§2.4, 7), then two namer items of 4 tests each (§2.6, §2.7).

## 1. Attribution

| where | tests | names |
|---|---:|---|
| excluded: macros #40 | 4 | quoted-sematics-1, quotes-add-erased, quotes-reflection, 25333 |
| excluded: research 27/28 singles | 9 | tuples, i4364a, i19641, i25000, i25000b, extension-methods, instances-anonymous (same `x.compareTo(y)` shape), i16065, manifest-summoning |
| #41 GADT | 4 | gadts, i14983, i16785, i10511 |
| #120 match types | 6 | anon-mirror-gen-local, i12328a, i12328b, 9416, i7868, typelevel-peano |
| #127 type tests | 5 | fully-abstract-nat, -4, -5, -6, -7 |
| #129 mirrors | 8 | i13332, i13332a, i13332anonprod, i13332shapeless-b, i14540-priorRun, deriving, deriving-from-product-typed, unroll-multiple |
| #139 / #146 named tuples | 5 | named-tuple-ops, named-tuples-xxl, named-tuples-strawman-2, namedtuples-buildcolumn, i22150 |
| #147 divergence measure | 4 | byname-implicits-17, generic, i11542a, i11563 |
| #122 block types | 4 | i13703, i14340, i24056, selectable-new |
| `compiletime.testing.typeChecks` (#130: needs errors, not refusals) | 9 | i11630, i12656, i18150, i22968, i7487, i7488, scala-tests-typeChecks, scala2-library-test, typeCheckErrors |
| out: XML, `3.3`, `into`, erased, `throws`, multi-spreads | 17 | i16458, i16459, xml; drop-apply-optimization; convertible; erased-machine-state, i13691, i23305, i16943, safeThrowsStrawman, safeThrowsStrawman2; i13691b, i13961a, saferExceptions, i15998; i11419, spreads-subtyping, spread-patterns |
| image / VM natives | 5 | t13033, t2849, t7336, t8601c, i4205 |

The other 198 split into the groups of §2 (≈ 75 tests) and a tail of ≈ 120 tests in ≈ 90 texts, of
which §3 lists the clusters of 2–4.

## 2. Groups, ranked by tests per line

| § | group | tests | lines | scivolo | dotty |
|---|---|---:|---:|---|---|
| 2.1 | Mirror type patterns in inline matches | 12 | ~25 | `Y/type_tests.rs:142`, `Y/patterns.rs:37-46` | `X/TypeTestsCasts.scala:160-170` |
| 2.5 | `import language.future` | 4 | ~5 | `Y/mod.rs:306-309` | `C/SourceVersion` |
| 2.3 | annotated type trees in library inline bodies | 5 | ~25 | `Yu/tpts.rs:69` | `C/tasty/TreeUnpickler.scala` ANNOTATEDtpt |
| 2.4a | inline `this` proxies: generic classes, nested objects, outer `this` | 8 | ~80 | `Yi/this_proxy.rs:30-50`, `Yi/outer_this.rs` | `N/Inliner.scala:387-455` |
| 2.2 | pattern-bound type variables (`maximizeType`) | 7 | ~120 | `Y/unapply.rs:191-239` | `T/Inferencing.scala:488-530`, `T/Applications.scala:1924-1960` |
| 2.4b | local classes, types and imports in inline bodies | 7 | ~100 | `Yi/copy.rs:84-91,133-160` | `D/TreeTypeMap.scala:163`, `C/Symbols.scala:920` |
| 2.6 | parents giving a base class different type arguments | 4 | ~60 | `Yn/base_types.rs:82-106` | `C/SymDenotations.scala` `baseTypeOf`, `C/TypeComparer.scala` `glbArgs` |
| 2.7 | enums extending `java.lang.Enum` | 4 | ~60 | `Y/classes.rs:80-91,104` | `D/DesugarEnums.scala:170-200`, `X/SyntheticMembers.scala:104-160` |
| 3 | refinement clusters, adaptation and member-not-found leftovers, singles | ≈ 30 | — | `Y/refined_types.rs`, `Y/adapt.rs`, `Y/select.rs` | — |

### 2.1 Mirror type patterns in inline matches (12 tests, ~25 lines)

All twelve stop at the same shape [V dumps]: `inline m match { case s: Mirror.SumOf[T] => …; case p:
Mirror.ProductOf[T] => … }` in a derivation method (i11050, i11174, i11174local, i11961, i12052,
i12328, i13146, i13146a, i9011, i9473, typeclass-derivation-doc-example, typeclass-derivation2d).
`Mirror.SumOf[T]` dealiases to a refinement `Mirror.Sum { type MirroredType = T; … }`:
`is_testable_type` is false (not a class), `try_with_type_test` finds no `TypeTest`, and
`is_erased_test_type` (`Y/type_tests.rs:142-151`) knows abstract types only, so `Y/patterns.rs:43`
refuses [V]. dotty's typer accepts any `Typed` pattern; checkability is decided at erasure
(`X/TypeTestsCasts.scala:160-170`): a `RefinedType` recurs on its parent and warns "it's a
refinement type" unless `hasMatchingMember` holds; the body of an inline method is erased without
the check (`Erasure.typedDefDef`, inline methods are effectively erased) [V].

Port: in `is_erased_test_type`, a `Type::Refined(parent, ..)` arm (`crates/tasty/src/types/mod.rs:56`)
that strips refinements and accepts a testable or erased-testable parent; `warn_unchecked_type_test`
takes the reason ("it's a refinement type" for this arm; the existing text for abstract types) [V
sites, I fix]. The inliner reduces the pattern with `is_subtype(scrut, tpe)` against the mirror's
refined type already (`Yi/reduce/patterns.rs:25-43`) [V]. Risks: the warning must not fire inside
inline bodies (dotty erases them) — `erasure_diags` (`Y/mod.rs:218`) is appended unconditionally at
`crates/typer/src/lib.rs:164`, so skip the warning while typing an inline method's body; a probe of
i9011 under `just typer-compare` for the warning count. Adjacent to #127 (type tests), disjoint file.

### 2.2 Pattern-bound type variables (7 tests, ~120 lines)

`Y/unapply.rs:231` refuses an uninstantiated invariant variable whose bounds differ after
`constrain_pattern` [V]: enum-Option / enum-Option1 (`case Some[T](x: T) extends Option[T]` against
`Option[String]`), enum-nat (`case Succ[N <: Nat](n: N)`), enum-approx (`Identity[T]() extends
Fun[T, T]`), i5077, i14693 (`case Array(i: Long)`), tuple-accessors. dotty's `maximizeType`
(`T/Inferencing.scala:488-530`): a variable with variance 0 whose `hi` is not `<: lo` and not final
becomes a fresh pattern-bound type symbol (`newPatternBoundSymbol`, bounds substituted), the
variable instantiated to it; `addBinders` (`T/Applications.scala:1924-1935`) wraps the `TypeApply`'s
argument in `Bind(sym, Ident)`; `typedUnApply` `:1956` [V]. Port: in `pattern_type`, replace the
refusal by a `CASE` type symbol owned by the case's owner with the variable's bounds, instantiate the
variable to it, return the binders to `typed_unapply`, which binds them on the `fun`'s type
arguments; the inliner's `reduce_pattern` and the lowering erase the symbol to its upper bound
(`crates/tasty/src/erasure`, as it erases abstract types) [I]. #127 item 2 names this site.

### 2.3 Annotated type trees in library inline bodies (5 tests, ~25 lines)

`Yu/tpts.rs:69` refuses six tags at once; breaks, loops, rescue-boundary, runtimeChecked and
runtimeChecked-2 expand `scala.util.boundary` / `runtimeChecked`, whose bodies carry `case ex:
Break[T] @unchecked` and `x.type @RuntimeChecked` — ANNOTATEDtpt [V sources, I tag: confirm with
a TASTy dump of `scala.util.boundary`]. Port: `ANNOTATEDtpt => { let arg = self.tpt(k[0])?; let
annot = self.term(k[1])?; … T::Annotated { arg, annot } with Type::Annotated(tpe, annot_tpe) }` as
`Y/type_trees.rs:113` builds it from source; `self.term` is `Yu/terms.rs:14` [V]. The other five
tags stay refused.

### 2.4 Inliner: `this` proxies (8 tests) and local classes (7 tests)

**a. `this` proxies, ~80 lines.** `Yi/this_proxy.rs:39,42,47` refuse an inline method of a nested
object (i11914, i11914a, i15317), of a generic class (better-fors-map-inlined, genericValueClass,
i2895a, i8530-b) and a body referring to an outer `this` (i15374) [V sites]. dotty's
`computeThisBindings` (`N/Inliner.scala:387-455`) binds one proxy per enclosing class, innermost
first, each to the call's prefix or to `outerSelect` hops from the last proxy; a generic class's
proxy is typed `C[args]` from the prefix's type (`adaptToPrefix`), a module class's proxy to the
module's path [V]. Port in `this_proxy.rs`: lift the module and type-parameter bans, type the proxy
by the prefix's widened type; `outer_this.rs` chains the outer proxies. Research 27 §2.2 I2 estimated
~40 for the objects alone.

**b. Local classes, ~100 lines.** `Yi/copy.rs:89-91` refuses `TypeDef`/`Template`/`Import` in a
copied body [V]: an anonymous class in an inline argument (i24357 `throw new E1 {}`, implicitFuns2,
t5665), a local class or type in the body (i13334, toplevel-mixed, typeclass-derivation1,
typeclass-derivation2b). dotty's `TreeTypeMap.transformDefs` (`D/TreeTypeMap.scala:163`) →
`mapSymbols` (`C/Symbols.scala:920`) copies the class symbol, its constructor and members with the
owner map, then transforms the template. Port: `fresh_symbols` (`copy.rs:139`) gains class symbols —
`declared_fresh` for a `Kind::Class` also declares fresh members (decls, primary constructor, self)
owned by the copy, maps their infos after all are made, and `copy` enters the `Template` with the
fresh class as `new_owner`; `Import` is copied as is (its expression retyped by the map) [I].

### 2.5 `import language.future` (4 tests, ~5 lines)

`Y/mod.rs:307` refuses `v.requires_new_syntax()` (`crates/syntax/src/config/version.rs:67`): four
tests only import `language.future` (given-disambiguation, i10930, i21213, i21213-min) [V]. Port:
accept `future` with 3.9 semantics (drop the `requires_new_syntax` clause). Risk: dotty under
`future` rejects old syntax (`_` wildcards, `with` types) that we would still accept — negative
tests only; note it in `docs/conformance.md`.

### 2.6 Parents giving a base class different type arguments (4 tests, ~60 lines)

`Yn/base_types.rs:82-106` `same_base_type` gives up (`None`) on a variant argument or two different
non-class arguments [V]: CollectionTests (`Seq[+A] extends Iterable[A] with SeqLike[A, Seq]`),
i10068d (`(() => C) with SAM_F`), iterables (`StrictOptimizedIterableOps[Int, Iterable,
Iterable[Int]]`), t2857 (`HashMap[Int, Set[String]] with MultiMap[Int, String]`). dotty's
`baseTypeOf` folds the parents' base types with `&` (`C/SymDenotations.scala` `foldGlb`), and
`AndType` of two applications of one class merges arguments by variance (`C/TypeComparer.scala`
`glbArgs`: covariant `&`, contravariant `|`, invariant equal or kept apart) [V dotty]. Port: a
`join_base_type` in `base_types.rs` applying `glbArgs`' rule to the two applications, `None` only
for an invariant mismatch that is not an error; `crates/tasty`'s `base_type` of the class must
compute the same join for the class's own base type, else the namer accepts what the subtyper
later denies [I].

### 2.7 Enums extending `java.lang.Enum` (4 tests, ~60 lines)

`Y/classes.rs:85-90` counts `java.lang.Enum`'s final `ordinal()` as the enum's own and `:104` refuses
(enum-constrs, enum-custom-toString, enum-ordinal-java, enum-values-order) [V]. dotty: cases of a
Java enum pass name and ordinal to `java.lang.Enum`'s constructor and define no `ordinal`
(`D/DesugarEnums.scala:170-200`, the `_$ordinal` comment at `:178`); `SyntheticMembers` skips
`ordinal`, `toString` and `hashCode` for `isJavaEnumValue` (`X/SyntheticMembers.scala:104-160`) [V].
Port: skip members owned by `java.lang.Enum` in `defines_enum_ordinal` (~5), then the case creator
and the synthetic member set for Java enums in `crates/syntax/src/desugar/enums.rs` and the
lowering's `SyntheticMembers` counterpart (~55) [I].

## 3. The tail: clusters of two to four

- **Refinements** (6, `Y/refined_types.rs:14,48,62,66`): "refines no type member" (i22649,
  implicit-functors2), "refers to the refined type's members" (i23409, typeclass-derivation2a),
  "structural refinement of an existing member" (i8073, i8073b, research 28 G) — one probe pass
  of `typed_refined_type` against dotty's `typedRefinedTypeTree` (`T/Typer.scala`), ~60 lines [I].
- **Adaptation leftovers** (`Y/adapt.rs:124`; research 28 §2 covers i14150, i9530, t3199b,
  string-context-implicits-with-conversion): new singles context-functions (`m.trans(m.a)`),
  enum-Tree (`case True => true`), enums-precise, fully-abstract-interface, lst (`newElems(i) =
  op(elem(i))`), t7120, transparentArrowAssoc (`Numeric.BigIntIsIntegral -> Ordering.BigInt`),
  unroll-clause-interleaving, unroll-value-class; inline-numeric fails for four reasons at once
  [V dumps]. Probe before coding, as research 28 did.
- **Member not found** (`Y/select.rs:97-154`): caseclasses (research 28 P), i23279 (§2.4b), instances
  (`case (x :: xs1, y :: ys1)` under a type-lambda given), i23245b [V dumps].
- **Dependent function types** (4): eff-dependent, ift-return, i19629 ("type tree ValDef",
  `Y/type_trees.rs`), t1939 (`Y/closures.rs:133`) — dotty `typedDependent`, ~80 lines.
- **Parent arguments inferred from `pt`** (3, `Y/type_trees.rs:137`): i4364b `new Consumer { … }`,
  type-test-binding `new TypeTest { … }`, tyql — dotty `typedNew` → `inferTypeParams`, ~30.
- **Import bounds** (3, `Y/import_tree.rs:66`): i18183.given, i18183.mixed, implied-for, ~40.
- **By-name method values** (3, `Y/eta.rs:70`): i20095, i5976, t7899, ~20 + lowering.
- **Applications** (3): creator-applys (`B[String]()`), i11583 (right-associative extension with a
  using clause), unapply-tparam (`case Foo[Int](x)`) — research 27 §2.3 V2 leftovers.
- **Implicit search** (`Y/implicits/`): no implicit argument — i17115, i23245a, implicit-alias and
  implied-specifity-2 (research 28 P), tupled-function-untupled (research 28 S); "implicit search
  without the call's context" (`Yi/host.rs:39`: erased-inline-product-3, i11542, i15988b,
  sip23-valueof — a `summonInline` reached without a host context, probe); ambiguous conversions
  (i3539, t8280, targetName-separate); extension views (`Y/implicits/extension_views.rs:29,68,74`:
  extension-specificity2, extmethod-overload, extmethods2, fragables-extension, i9928).
- **Singles with a known site** [V sites]: `@static` with a non-literal (i5924b, i5924c, statics),
  `inline expansion of a type referring to a binding` (opaque-inline2, tuple-map,
  `Yi/inline_call.rs:70`), generic tuples over 22 (i13968, tuple-product, `Y/select.rs:29`),
  HK implicit variables (i13986, i13987, `Y/implicits/unsettled.rs:29`), implicit imports from
  values (range, typelevel-numeric), opaque types outside objects (i12914, option-extract, i6716),
  user `Selectable`s (i17761, structural-contextual), structural `using` refinements (structural,
  structural-compat), ambiguous references (t4537, triemap-hash), `codeOf` (i13181, i13183).

## 4. Order and conflicts

Order: §2.1 → §2.5 → §2.3 → §2.4a → §2.2 → §2.4b → §2.6 → §2.7, then §3's probes. The first three
are disjoint (`Y/type_tests.rs`, `Y/mod.rs`, `Yu/tpts.rs`) and fit one session; §2.4a and §2.4b share
`Yi/` with #82 and research 26's macro work — one coder for the inliner; §2.2 shares `Y/unapply.rs`
with #127 — #127 first or the same coder; §2.6 touches `crates/tasty`'s base types, as #120's
erasure does — sequential. Gates as research 27 §3: `just gates`, `typer-compare` on the probes
that become golden inputs, `dotty-lower-report`, `snapshot-diff programs lower typer`.
