# 39 — Refusals in dotty's tests/run and tests/run-macros after 2026-10-27

Date: 2026-10-29. Claims are [V] verified by reading the code or running it, [I] inferred. Paths
as in research 38: `T/` = `ref/scala3/compiler/src/dotty/tools/dotc/typer/`, `Y/` =
`crates/typer/src/typer/`, `Yn/` = `crates/typer/src/namer/`, `Yi/` = `crates/typer/src/inlining/`,
`Yu/` = `crates/typer/src/unpickle/`, `L/` = `crates/lower/src/phases/`, `M/` = `crates/macros/src/`,
`O/` = `overlay/src/scala/quoted/runtime/impl/`, `Qi/` = `ref/scala3/compiler/src/scala/quoted/runtime/impl/`.
Measured on main f3907e5d (merged through 2026-10-29, the typer/lowering/macro batches of 10-26
and #168 of 10-27) with `just dotty-lower-report` (`tmp/dotty/3.9.0-0.5.12/lower-results.tsv`):
**1401 pass** of 1713 (research 38: 1321 of 1711; the main checkout's run of 10-27: 1399), 92 typer
refusals in 57 distinct texts, 13 lowering, 0 parser, **0 wrong output**, 0 crashes, 32 fail in
the VM also under Scala Native, 175 skipped [V]. `just dotty-lower-report "" run-macros`
(`lower-results-run-macros.tsv`): 321 tests, **213 pass** (research 38: 188), 94 typer refusals in
61 texts, 2 lowering, 2 frontend crashes, 1 VM crash, 1 earlier group, 8 skipped [V]. Against the
10-27 run: 2 improved (i3339, i7991: their earlier groups compile now), none worsened [V diff].
Both reports finished within minutes on this machine (the release build was cached); no sampling. `just typer-report`
(tests/pos) is not repeated: this worktree has no oracle trees, research 38's 1893 stands.
Method: `tmp/r39-rows.py` groups the stops by reason and greps each text's site in `crates/`;
`tmp/r39-attrib.py` runs `just typer-dump` per refused single-file test and maps the offset to
its source line (directory tests and `run-macros` by their reason text and sources) [V].

**Recommendation.** Of research 38's six batches A–F only singles are left (A 9, B 15, C 3, E 17,
N 5 tests); the macro batch G did its reflect operations but `run-macros` still stops in 100 tests,
53 of them in a reflect member refused by name or a printer. The next batch for one coder is five
stacked MRs disjoint by file (§4): **1** macro options and `Symbol.tree` of classpath symbols (9
tests, ~150 lines; accepting `-Yretain-trees` alone passes tasty-load-tree-2 [V]), **2** namer
singles — nested `inline val`, top-level opaque types, `` `<empty>` ``, shared pattern definitions,
exports, generic number literals (10 tests, ~150), **3** types and implicit scope — refinements of
existing members, higher-kinded alias overrides, scopes of abstract-typed paths, #123 (11 tests,
~170), **4** `Printer.TypeReprStructure` and the `TreeStructure` gaps (6 tests, ~130), **5**
`typeChecks` leftovers (9 tests, ~150). After it: the reflect singles and printers (42 tests,
~800), the implicit search shapes and adaptation probes (B/E), the inliner leftovers with #165.

## 1. Attribution

### 1.1 tests/run: 105 frontend stops

| where | tests | names (site) |
|---|---:|---|
| lowering #165 (`L/`), the 13 ROADMAP counts | 13 | i12328a, i13332anonprod, i13332shapeless-b (`L/outer_args.rs:109`); i5924b (`L/static_members.rs:32`); context-functions (`L/uninitialized_defs.rs:36`); anon-mirror-gen-local (`L/anon_mirrors.rs:35`); i3539 (`codegen/cast.rs:135`); enums-thunk (`L/local_free.rs:158`), i13332 (`:114`), poly-kinded-derives (`:173`); i2266 (`codegen/callee.rs:65` "inline call not expanded"); i7359 (`L/sam_classes.rs:32`); CollectionTests (`L/super_accessors.rs:185`) — 7 of them were typer refusals in research 38 [V diff] |
| inliner (`Yi/`, `Y/accessor_defs.rs`; §2.16) | 8 | i12914, i13461-b, loops-alt (`Yi/opaque_proxies.rs:40,49`); erased-inline-product-1 (`Yi/reduce/bindings.rs:47`), -2 (`Yi/compiletime.rs:187`); typelevel-peano (`Yi/reduce/patterns.rs:28`); lst (`Y/accessor_defs.rs:134`, `multi[U](newElems)`); i23279 ("member not found" `defn.Lifecycle.FromZIO`, a local object in an inline body) |
| macros (§2.8, 2.9, 2.12) | 13 | `typeChecks` 7: i11630 (`Yi/mod.rs:257`), i12656 (`Yi/compiletime.rs:60`), i18150 (`Y/selves.rs:93`), i22968 ("reference to Foo"), scala-tests-typeChecks, scala2-library-test, typeCheckErrors (`Yi/type_checks.rs:30`); i8530, i8577i (`Y/transparent.rs:75`, `Y/unapply.rs:65`); quoted-sematics-1 (adaptation at `case (Nat(n), PNat(n2))`); i22649 (`TypeReprStructure`); quotes-add-erased (`AnnotatedType.apply`); quotes-reflection (`TermParamClause` method missing in the VM) |
| javalib (#168) | 2 | t2849, t7336 (`Runtime.totalMemory`) |
| #41 GADT | 4 | gadts, i14983, i16785 (`Y/unapply.rs:251`), enum-Tree (adaptation at `case True => true`) |
| out: XML, `into`, spreads, modularity, capture checking, `-source:3.3`, package object values, `@specialized`, `caps.unsafe` | 16 | i16458, i16459, xml; convertible; i11419, spreads, spreads-subtyping, spread-patterns; i15840, i3920, given-disambiguation; errorhandling; drop-apply-optimization; pkgobjvals; t4770; i16943 |
| A types (§2.5, 2.6, 2.14) | 9 | i8073, i8073b, manifest-summoning (`Y/refined_types.rs:48`); decorators, i22888 (`Y/type_lambdas.rs:78`); i13332intersection (`Y/constructors.rs:93`); i15998 (`Y/stats.rs:117`); named-tuples-strawman-2 (`Y/type_args.rs:40`); tyql ("member not found" `c.zipCode`, a `Selectable` with `Fields`) |
| B implicits (§2.4, 2.7, 2.13) | 15 | fully-abstract-nat-6, -7 (`Y/implicits/scope_paths.rs:69`); distinct, enrich-gentraversable, i7960 (`search.rs:162`, #123); i13986, i13987 (`unsettled.rs:29`); Providers (`search.rs:109`); byname-implicits-6 (`search.rs:72`), -28 (`dictionary.rs:89`), -17 ("no implicit argument" `Show[List[Int]]`, #147); i17115 (`measures.rs:9`, #147); tagless (`adapt_by_result.rs:53`); i13332shapeless (`Generic.materialize[…]`); generic ("no implicit argument", directory) |
| C patterns and mirrors (§2.18) | 3 | i5077 (`Y/unapply.rs:258`); 16405 ("unresolved name a" in `case x: ((a, b) => R)`, an inline match); unroll-multiple (`Y/implicits/product_mirrors.rs:66`) |
| E adaptation, application (§2.15) | 17 | adaptation: i14150 (`head.get *: tail.get`), inline-numeric (`new ReverseOrdering(f) {}`), fully-abstract-nat-5 (`def refined = this`), t3199b (Java varargs); "application of a non-method": fully-abstract-nat (`n.intValue()`, `BigInt.intValue` overrides Java's), i11583 (`Y/apply_insertion.rs:64`), i13691b, i13961a (`summon[Decoder[Int throws Exception]]()`); i4364a (`Y/eta.rs:186`); i8903 (`Y/named_args.rs:181`); tupled-function-apply (`:40`); i4600b (`Y/apply.rs:130`); iarrays (`Y/overloads.rs:106`); i6816 (`Yn/params.rs:129`); i13332a (`` `<empty>`.Opt ``); genericNumLits (`Y/literals.rs:45`); targetName-separate (`Y/constructor_proxies.rs:35`) |
| N namer (§2.2, 2.3) | 5 | typeclass-derivation2a, 2d, 3 (`Yn/inline_checks.rs:32`); Parser (`Yn/export_paths.rs:21`); i6716 (`Yn/checks.rs:54`) |

Research 38's named tests that pass now [V tsv]: sticky-extmethod (the wrong output), i9530,
iterator-from, i13433b, extmethods2, fully-abstract-interface, namedtuples-buildcolumn, i7487,
i7488, i14540-priorRun, i10511, i10930, i15302a, 9416, caseclasses, the `Selectable` block types,
the structural calls, i22345, i22150, i9928, i23897, i23409, and the 19 of the lowering track but
i13332, i12328a, i13332anonprod, i13332shapeless-b, i5924b, i12914, i13461-b, erased-inline-product-1.

### 1.2 tests/run-macros: 100 stops

| where | tests | names |
|---|---:|---|
| options and `Symbol.tree` (§2.1) | 11 | `-Yretain-trees` 6 (tasty-definitions-1/-2/-3, tasty-load-tree-1/-2, tasty-extractors-owners), `-Xread-docs` 2 (i12351, i12352: `Symbol.docstring`), `-Xcheck-macros` (i17445-copy), `-Xmacro-settings` (Xmacro-settings-simple), i9206 (`Symbol.tree` of a classpath symbol) |
| printers (§2.8, 2.10) | 17 | `Printer.TypeReprStructure` 4 (tasty-extractors-1/-2/-3/-types), `TypeRepr.show` of an unprinted type 3 (i7987, tasty-macro-positions, type-show; `crates/tasty/src/types/show_source.rs:12`), `Type`/`TypeRepr.toString` 3 (i22395, i24006, self), `Expr`/`Tree.toString` 2 (i8746, i8746b), `codeOf` unprinted 2 (i8306, inline-beta-reduce-polyfunction; `Yi/code_of.rs:17`), `TreeCode` unprinted (term-show; `M/show_code/mod.rs:20`), `TypeReprShortCode` (i19905), `TreeAnsiCode` (i6270) |
| reflect members refused by name or missing (§2.11) | 29 | `Symbol.newClass` with traits or type parameters 3 (`Yi/reflect_classes.rs`), `TypeLambda.paramVariances` 2, `Implicits.searchIgnoring` 2 (`O/ReflectImpl.scala:86`), `Select.overloaded` 2 (`M/reflect/overloads.rs:47`), `MethodType.apply`/`.methodTypeKind`/`.param` 3 (i22260, reflect-method-type-kind, tasty-create-method-symbol), `ConstantType.apply`, `Flags.Deferred` (`O/ReflectFlags.scala:22`), `Symbol.newBind`, `Symbol.newVal` flags (`Yi/reflect_defs.rs:35`), `TypeRepr.substituteTypes`, `Symbol.signature` (i7025b), `Symbol.children` of a library class (`M/reflect/members.rs:151`), `Symbol.methodMembers` of a non-class (`:98`), `Symbol.paramSymss` (`:85`), annotations outside the unit (`M/reflect/definitions.rs:22`), members of a non-class type (`M/reflect/type_members.rs:64`), an operation on `noSymbol` (`M/reflect/mod.rs:96`), `Apply` arity (`Yi/reflect_applies.rs:22`), VM "no method" 3 (`Class.getCanonicalName` inline-case-objects; `TermParamClause` i12021; `GivenSelector` i21225), erased macro parameters (macro-erased, `M/lib.rs:55`) |
| splices, patterns, annotations, crashes (§2.12) | 13 | a splice other than a macro call 2 (`Yi/macro_impls.rs:22`), higher-order holes of polymorphic methods 2 (`Yi/quote_matcher/open.rs:42`), inline-varargs-1 (`:68`), expr-map-1 (`Yi/retype.rs:157` "inline body with a return or label"); annot-add-global-class (`Yi/macro_definitions.rs:57`), annot-concrete-class (`Y/abstract_members.rs:53`: the annotation implements the member), annot-mod-class-add-local-class (`codegen/callee.rs:56`), annot-java-tree (`SuppressWarnings`, a JDK stub: tooling); crashes tasty-interpolation-1, xml-interpolation-2 ("Expected statically known StringContext"), reflect-isFunctionType (VM assertion) |
| §1.1's groups, counted here | 28 | adaptation 6 (beta-reduce-inline-result, enum-nat-macro, quote-matcher-symantics-3, quote-whitebox, refined-selectable-macro, whitebox-inline-macro), "member not found" 3 (f-interpolator-tests, i10011, refined-apply), library inline bodies 4 (quote-match-poly-function-1, -1-regression, -2: `Yu/locals.rs:87`; i15968: `Yu/selections.rs:111`), top-level opaque types 2 (i18283, tasty-dealiasKeepOpaques), opaque-inline (`Y/implicits/scope.rs:94`), "local type escaping" 2 (quote-matcher-runtime `Y/closures.rs:125`, tasty-tree-map `Y/infer.rs:181`), quoted-ToExpr-derivation-macro (`product_mirrors.rs:131`), i4803c (`Y/inline_accessors.rs:155`), i10880 (`L/trait_params.rs:61`), reflect-typeChecks, reflect-inline (`typeChecks`), type-print (`Y/transparent.rs:80`), BigFloat (`Y/literals.rs:45`), exports (`Yn/export_members.rs:36`), lib-StringContext (a Scala 2 `macro` definition), tasty-overload-secondargs (`Y/overloads.rs:125`) |
| tooling, out | 2 | Xmacro-settings-compileTimeEnv (scala-cli lacks `-Xmacro-settings` for the earlier group), tasty-simplified (`-source:3.3`) |

## 2. Groups, ranked by tests per line

| § | group | MR | tests | lines | scivolo | dotty |
|---|---|---|---:|---:|---|---|
| 2.2 | nested `inline val` | 2 | 3 | ~20 | `Yn/inline_checks.rs:16,32` | `inlines/PrepareInlineable.scala:302` |
| 2.4 | implicit scope of abstract-typed paths | 3 | 3 | ~40 | `Y/implicits/scope_paths.rs:69`, `scope.rs:94` | `T/Implicits.scala:755` `addPath` |
| 2.3 | top-level opaque types | 2 | 3 | ~40 | `Yn/checks.rs:54` | `T/Namer.scala:1178` |
| 2.5 | refinements of existing members | 3 | 3 | ~40 | `Y/refined_types.rs:48` | `T/Typer.scala:2704` |
| 2.6 | higher-kinded alias over a type member | 3 | 2 | ~30 | `Y/type_lambdas.rs:78` | `T/Namer.scala:1124-1129` `TypeDefCompleter.typeSig` |
| 2.1 | macro options, `Symbol.tree`, `defn.AnyRefClass` | 1 | 9 (+2) | ~150 | `scivolo-lower.rs:101`, `M/reflect/definitions.rs:93`, `O/ReflectSymbols.scala:138`, `O/ReflectFlags.scala:89`, `O/ReflectImpl.scala:74` | `Qi/QuotesImpl.scala:2997,3227` |
| 2.7 | #123 candidates with open variables | 3 | 3 | ~60 | `Y/implicits/search.rs:162` | `T/Implicits.scala:1184` `typedImplicit` |
| 2.9 | `typeChecks` leftovers | 5 | 9 | ~150 | `Yi/type_checks.rs:30`, `Yi/compiletime.rs:60`, `Yi/mod.rs:257`, `Y/selves.rs:93` | `inlines/Inliner.scala` `Intrinsics` |
| 2.8 | `TypeReprStructure`, `TreeStructure` gaps | 4 | 6 | ~130 | `M/show_code/structure.rs`, `O/ReflectImpl.scala:34` | `Qi/printers/Extractors.scala:208` |
| 2.11 | reflect singles | — | 29 | ~500 | `O/`, `M/reflect/`, `Yi/reflect_*.rs` | `Qi/QuotesImpl.scala` (§1.2 lines) |
| 2.10 | other printers | — | 13 | ~300 | `M/show_code/`, `show_source.rs`, `Yi/code_of.rs` | `Qi/printers/SourceCode.scala`, `core/Types.scala` `toString` |
| 2.14 | types singles | — | 5 | ~150 | `Y/constructors.rs:93`, `Y/stats.rs:117`, `Y/type_args.rs:40`, `Y/select.rs`, `Yi/reduce/patterns.rs` | `T/Typer.scala:936` (`Fields`) |
| 2.18 | patterns and mirrors | — | 3 | ~80 | `Y/unapply.rs:258`, `product_mirrors.rs:66,131` | `T/Applications.scala:1718` |
| 2.12 | splices, patterns, annotations | — | 13 | ~400 | `Yi/macro_impls.rs`, `Yi/quote_matcher/`, `Yi/retype.rs`, `Yi/macro_definitions.rs` | research 26 P, B; ADR 0039 |
| 2.13 | implicit search shapes (probe) | — | 10 | ~300 | `Y/implicits/{unsettled,search,dictionary,measures,adapt_by_result}.rs` | `T/Implicits.scala` |
| 2.16 | inliner leftovers (with #165) | — | 14 | ~300 | `Yi/opaque_proxies.rs`, `Yi/reduce/`, `Y/accessor_defs.rs`, `Yu/locals.rs`, `Yu/selections.rs`, `Yi/retype.rs` | `inlines/Inliner.scala` |
| 2.15 | adaptation and application (probe) | — | 31 | — | `Y/adapt.rs:129`, `Y/apply*.rs`, `Y/eta.rs`, `Y/named_args.rs` | — |
| 2.17 | lowering | #165 | 15 | — | `L/` (§1.1) | — |

### 2.1 Macro options, `Symbol.tree`, `defn.AnyRefClass` (9 tests, ~150 lines)

`scivolo-lower` drops options without effect on code but keeps `-Yretain-trees` as a refusal when
macros run (`crates/lower/src/bin/scivolo-lower.rs:101`) [V]. Accepting it always, measured on the
six tests [V run]: tasty-load-tree-2 **passes**; tasty-definitions-2, -3 and tasty-load-tree-1
stop at "`Symbol.tree` of a symbol without a definition in the unit" (`O/ReflectSymbols.scala:138`:
`M/reflect/definitions.rs:93` searches the unit's typed trees only, so the earlier group's
`Foo.foo` has none); tasty-definitions-1 at `defn.AnyRefClass` (`O/ReflectFlags.scala:89`, dotty
`Qi/QuotesImpl.scala:3227` returns `AnyRefAlias`: 3 lines); tasty-extractors-owners at
"`TreeStructure` of a tree the frontend does not print" (§2.8). dotty's `Symbol.tree` is
`FromSymbol.definitionFromSym` (`:2997`), which for classpath symbols unpickles the TASTy
definition [V]. Port: in `definition`, fall back to `Yu/` for symbols with a TASTy address, which
reads `ValDef`/`DefDef` (`Yu/defs.rs`) and class bodies (`Yu/class_defs.rs`) for library inline
bodies already — i9206 too [I ~100]. `-Xcheck-macros` has no effect on code (1 line; i17445-copy
has no other known stop [I]). `-Xmacro-settings:…` goes into `CompilationInfo.XmacroSettings`
(`O/ReflectImpl.scala:74`): the lower binary keeps the option's values and hands them to the VM
macros runner [I ~30]; Xmacro-settings-compileTimeEnv additionally needs `scripts/dotty_sources.py`
to pass the option to scala-cli. `-Xread-docs` and `Symbol.docstring` (`O/ReflectDeclarations.scala:40`)
depend on the earlier group's TASTy carrying comments, which scala-cli writes only with that option
[I]: measure before porting.

### 2.2 Nested `inline val` (3 tests, ~20 lines)

typeclass-derivation2a, 2d, 3 define `inline val size = constValue[Tuple.Size[Elems]]` inside a
`transparent inline def` [V dumps]; `Yn/inline_checks.rs:16` sets `nested` for any owner that is an
inline method and `:32` refuses every inline definition under it. dotty refuses only nested inline
*methods* (`inlines/PrepareInlineable.scala:302-304` `checkInlineMethod`) [V]; the corpus compiles
the values. Port: refuse under `nested` only when `kind == Kind::Def`; the value then types as an
inline value whose right-hand side is constant after inlining (dotty: `Checking.checkInlineConformant`
is lenient inside inline methods [I]) — check `Y/defs.rs:37`'s purity test accepts `constValue[…]`
there and that `Yi/reduce/` folds the binding when the method is inlined [I].

### 2.3 Top-level opaque types (3 tests, ~40 lines)

i6716 (`tests/run`), i18283, tasty-dealiasKeepOpaques (`run-macros`) stop at `Yn/checks.rs:54`
"top-level opaque type": the owner is the named package object [V]. dotty's `Namer` completes a
top-level opaque alias as a member of the package object, whose `this` is the opaque's scope
(`T/Namer.scala:1178`; research 38 §2.12) [V name]. The existing opaque handling for classes and
objects applies once the package object counts as the enclosing class for `sees_opaques` [I].

### 2.4 Implicit scope of abstract-typed paths (3 tests, ~40 lines)

fully-abstract-nat-6, -7: `val s = b.asInstanceOf[x0.Refined]; Some(safeDiv(a, s))` — the
implicit scope of the path `s` whose type is the abstract member `x0.Refined` stops at
`Y/implicits/scope_paths.rs:69` "implicit scope through a value of no class type" [V dumps]; dotty's
`addPath` (`T/Implicits.scala:755`) adds the members of `pre.underlying`'s base classes for any
path [V]. Port: widen to the upper bound and take its base classes instead of `class_symbol`.
opaque-inline (`scope.rs:94` "implicit scope of an abstract or opaque type with a companion") is
the same file set: add the companion of the opaque alias's owner object [I].

### 2.5 Refinements of existing members (3 tests, ~40 lines)

`Y/refined_types.rs:48` refuses a term refinement whose name the parent already has: i8073, i8073b
`CC { val a: aa.type }`, and manifest-summoning's `((B {def b: 0} & C) & …) {def c: 1}` [V dumps].
dotty's `typedRefinedTypeTree` (`T/Typer.scala:2704`) accepts any refinement a parent member admits
[V]. Port: build the `Refined` type and check the refinement's info conforms to the parent member's
(`checkRefinementNonCyclic` / `RefChecks` leave the rest to the subtyper); member lookup already
prefers the refinement for type members [I].

### 2.6 Higher-kinded alias over a type member (2 tests, ~30 lines)

i22888, decorators: `type A[T] = Int` implementing `type A[T]` of the parent stops at
`Y/type_lambdas.rs:78` [V]. dotty's `TypeDefCompleter.typeSig` (`T/Namer.scala:1124-1129`) wraps
the right-hand side in `HKTypeLambda.boundsFromParams` whatever the alias overrides, and the
override check compares the lambdas [V]. Port: take the non-overriding branch for overriding
aliases too.

### 2.7 #123: candidates with open type variables (3 tests, ~60 lines)

distinct (`util.Random.shuffle(alphaList)`), enrich-gentraversable (`l.filterMap(…)`), i7960
(`Future.sequence(Seq(…))`) stop at `Y/implicits/search.rs:162`: the candidate's value type keeps an
undetermined type constructor variable after `with_implicit_args` found no arguments to apply [V].
dotty's `typedImplicit` (`T/Implicits.scala:1184`) adapts the candidate against the expected type
and interpolates its type variables before ranking [V]. Port: run `interpolate` on the unapplied
candidate too (the `applied != tree` branch above it) and refuse only variables that stay open,
#123's option 3.

### 2.8 `TypeReprStructure` and the `TreeStructure` gaps (6 tests, ~130 lines)

tasty-extractors-1/-2/-3/-types, i22649 (`tests/run`) and, after §2.1, tasty-extractors-owners.
dotty's `Extractors.showType` (`Qi/printers/Extractors.scala:12`, `visitType` `:208-~300`) prints
every `TypeRepr` case as a constructor tree [V]; ours has `TreeStructure` (`M/show_code/structure.rs`,
218 lines) and refuses `TypeReprStructure` by name (`O/ReflectImpl.scala:34`) [V]. Port: a sibling
`M/show_code/type_structure.rs` over `tasty::Type` (~100 lines), and the tree printer's missing
cases found by the owners test [I].

### 2.9 `typeChecks` leftovers (9 tests, ~150 lines)

ADR 0053's intrinsic types the code string in a throwaway scope. Left [V texts]: a non-constant
argument that is an inline parameter (typeCheckErrors, reflect-inline: fold after the argument's
inlining), `typeCheckErrors` of code with errors (i12656: collect the diagnostics instead of
failing), a call of an inline method defined in the string (i11630, `Yi/mod.rs:257`), a class not
conforming to a self type inside the string (i18150), a reference to a class of the string from
the result (i22968), the search failures of scala-tests-typeChecks and reflect-typeChecks, the
adaptation in scala2-library-test (a directory test; its stop is unlocated).

### 2.10 Other printers (13 tests, ~300 lines)

`TypeRepr.show` of an unprinted type (i7987, tasty-macro-positions, type-show:
`crates/tasty/src/types/show_source.rs:12`), `codeOf` of an unprinted argument (i8306,
inline-beta-reduce-polyfunction), `TreeCode` (term-show), `ShortCode` (short names of `TreeCode`),
`AnsiCode` (`SyntaxHighlight.scala`), and the five `toString` tests, which print dotc's internal
`Types.toString`/`Trees.toString` (i22395, i24006, self, i8746, i8746b: ~100 lines of a third
printer, last).

### 2.11 Reflect singles (29 tests, ~500 lines)

Each a thin operation in `O/` plus `M/reflect/` or `Yi/reflect_*.rs` (research 26 §3.7): §1.2's
list with sites. By tests per line first: `Symbol.newClass` with traits and type parameters (3),
`MethodType` (3), `paramVariances` (2), `searchIgnoring` (2), `Select.overloaded` without one most
specific alternative (2), then the singles. The three VM "no method" rows need overlay members
(`TermParamClause`, `GivenSelector`) or javalib (`Class.getCanonicalName`).

### 2.12 Splices, quote patterns, macro annotations, interpolation (13 tests, ~400 lines)

Research 26 P and B unchanged for the splices (`Yi/macro_impls.rs:22,68`) and higher-order holes
(`Yi/quote_matcher/open.rs:42`); expr-map-1 needs `return` in a retyped inline body
(`Yi/retype.rs:157`); annot-add-global-class a definition added to another owner, annot-concrete-class
the abstract-member check after the annotation ran (`Y/abstract_members.rs:53`); the two
interpolation crashes see a `StringContext.apply` tree the macro cannot match ("Expected statically
known StringContext": the `Apply` shape [I]); reflect-isFunctionType asserts in the VM.

### 2.13 Implicit search shapes (10 tests, ~300 lines, probe first)

Research 38 §2.7 minus the done ones: i13986, i13987 (`unsettled.rs:29`), Providers (local givens
defined later in the block, `search.rs:109`), tagless (`adapt_by_result.rs:53`), byname-implicits-6
(`search.rs:72`), -28 (`dictionary.rs:89`), -17 and i17115 (#147: `measures.rs:9` and a by-name
`Show[R]` loop), i13332shapeless (`Generic.materialize[Defs.Sum, …]`), generic (directory; its
`shaped`/`unfolds` instances). Probe each with `typer-compare` before coding.

### 2.14 Types singles (5 tests, ~150 lines)

i13332intersection (`Y/constructors.rs:93`: bounds through a `this` prefix count as dependent),
i15998 (`Y/stats.rs:117`: the extension `isRotationOf` with `CC[B] <: SeqOps[B, CC, CC[B]]` is left
unapplied), named-tuples-strawman-2 (`Y/type_args.rs:40`: the higher-kinded parameter `P` passed on
inside a match-type case), tyql (`c.zipCode` on a `Selectable` with a `Fields` type member;
dotty `T/Typer.scala:936-944` selects through `Fields`), 16405 (`case x: ((a, b) => R)` binds type
variables in an inline match: `Yi/reduce/patterns.rs:28` with C's pattern-bound types).

### 2.15 Adaptation and application (31 tests, probe first)

§1.1's E row and §1.2's adaptation, "member not found" and escaping rows. Cheap singles [I]:
fully-abstract-nat (`n.intValue()`: `()` on a parameterless method overriding a Java one; dotty
`adaptNoArgs` allows it), i13691b/i13961a (`summon[Decoder[Int throws Exception]]()`: `apply`
insertion when the receiver's type is a `throws` alias of a context function), i6816 (one copy of the
right-hand side per name in the desugaring), i13332a (`` `<empty>` ``: `defn.RootPackage.info.member`,
dotty `T/Typer.scala:1093`), genericNumLits and BigFloat (`FromDigits`, `T/Typer.scala:1165`),
targetName-separate (constructor proxies for classes of earlier groups). The rest as research 38
§2.11: i14150, inline-numeric, fully-abstract-nat-5, t3199b (javalib's Java varargs), i11583,
i4364a, i8903, tupled-function-apply, i4600b, iarrays, and the macro tests.

### 2.16 Inliner leftovers (14 tests, ~300 lines, with #165)

`Yi/opaque_proxies.rs:40,49` (i12914, i13461-b, loops-alt: an inline method of an object with
opaque types used from outside, `boundary.Label` aliases), `Yi/reduce/bindings.rs:47` and
`Yi/compiletime.rs:187` (erased-inline-product-1, -2), `Yi/reduce/patterns.rs:28` (typelevel-peano),
`Y/accessor_defs.rs:134` (lst: an inline accessor of `multi[U]` whose type mentions `U`),
i23279 (a local object's member in an inline body), i4803c (`Y/inline_accessors.rs:155`),
`Yu/locals.rs:87` (a closure of a parameterless local method in a library inline body: dotty's
`toFunctionType` of an `ExprType`), `Yu/selections.rs:111` (i15968), `Yi/retype.rs:157` (expr-map-1).
Research 38 gave `Yi/` to the lowering track; the ROADMAP's #165 list names none of these, so this
track takes them unless #165 claims them.

### 2.17 Lowering (15 tests): the lowering track's (#165), §1.1's first row plus i10880 and
annot-mod-class-add-local-class.

### 2.18 Patterns and mirrors (3 tests, ~80 lines)

i5077 (`Y/unapply.rs:258` pattern-bound type against a wildcard argument, #164 §2.2, #127),
unroll-multiple (`@unroll` mirrors, `product_mirrors.rs:66`), quoted-ToExpr-derivation-macro
(`:131` "Mirror of a singleton outside the subset").

## 3. The issues in scope [V tsv]

| issue | state |
|---|---|
| #164 | left: §2.2 pattern-bound types (i5077, 16405, typelevel-peano), §2.4b local classes in inline bodies (i23279, i4803c); §2.6 and §2.7 pass |
| #165 | the 13 lowering refusals of §1.1 (ROADMAP's count), i10880, annot-mod-class-add-local-class; the `Yi/` leftovers of §2.16 to agree |
| #166 | i9530, iterator-from, i16065 pass; left: i11583 (§2.15), manifest-summoning (now §2.5), i3539 (lowering: primitive casts), t4579 (#105, tests/pos) |
| #120 | no `tests/run` stop left but named-tuples-strawman-2 (§2.14); items 1–8 stay as listed |
| #122, #156 | typed-tree printing of tests/pos; no `tests/run` stop; not measured here |
| #127 | fully-abstract-nat (§2.15), -5 (adaptation of `def refined = this`), -6, -7 (§2.4); i13433b passes |
| #146 | namedtuples-buildcolumn passes; tyql's `Fields` selection (§2.14) |
| #155 | typeclass-derivation2a (now §2.2), fully-abstract-interface and extmethods2 pass; t4660 (tests/pos) |
| #123 | §2.7 |
| #147 | byname-implicits-17, -6, i17115 (§2.13) |
| #41 | gadts, i14983, i16785, enum-Tree: no batch |
| research 26 | W5 printers (§2.8, 2.10), W1/W2/W4 singles (§2.11), P/B (§2.12), C (§2.9), T (`-Xmacro-settings`, §2.1) |

## 4. Batches

The next batch, five stacked MRs for one coder, disjoint by file; order by tests per line, each
reviewed and gated (`just gates`, `just dotty-lower-report FILTER [run-macros]` on its tests,
`just snapshot-diff programs lower typer` without worsened rows) before the next starts:

| MR | branch | § | tests | lines | owns |
|---|---|---|---:|---:|---|
| 1 | `macro-options-1` | 2.1 | 9 (+2) | ~150 | `crates/lower/src/bin/scivolo-lower.rs`, `scripts/dotty_sources.py`, `O/ReflectImpl.scala:74`, `O/ReflectFlags.scala:89`, `O/ReflectSymbols.scala:138`, `O/ReflectDeclarations.scala:40`, `M/reflect/definitions.rs`, `Yu/defs.rs`, `Yu/class_defs.rs` (a `pub fn` reading one definition), the macro VM runner's settings |
| 2 | `namer-singles-3` | 2.2, 2.3, i6816, i13332a, Parser, exports, genericNumLits, BigFloat | 10 | ~150 | `Yn/inline_checks.rs`, `Yn/checks.rs`, `Yn/params.rs`, `Yn/export_paths.rs`, `Yn/export_members.rs`, `Y/idents.rs`, `Y/find_ref.rs`, `Y/literals.rs`, `Y/defs.rs:37`, `crates/syntax/src/desugar/` |
| 3 | `types-scope-3` | 2.4, 2.5, 2.6, 2.7 | 11 | ~170 | `Y/refined_types.rs`, `Y/type_lambdas.rs`, `Y/implicits/scope_paths.rs`, `Y/implicits/scope.rs`, `Y/implicits/search.rs` |
| 4 | `reflect-structure` | 2.8 | 6 | ~130 | `M/show_code/` (new `type_structure.rs`, `structure.rs`), `O/ReflectImpl.scala:34` |
| 5 | `typechecks-3` | 2.9 | 9 | ~150 | `Yi/type_checks.rs`, `Yi/compiletime.rs:60`, `Yi/mod.rs:257`, `Y/selves.rs:93` |

MR 1 first: its first commit (accept `-Yretain-trees` with macros) is one line and passes
tasty-load-tree-2 [V]; the `Symbol.tree` fallback is the only item above ~50 lines. MR 2 and 3 are
independent of it and of each other. MR 4 lands before any other printer work; MR 5 is the one
with probes (scala2-library-test's stop is unlocated) and may shrink.

Following batches, in order: **reflect singles and printers** (§2.11, 2.10; ~42 tests, ~800
lines, `O/` and `M/`, after MR 4), **splices, patterns, annotations** (§2.12, `Yi/macro_*`,
`Yi/quote_matcher/`, `Yi/retype.rs`), **implicits and types** (§2.13, 2.14, 2.18: `Y/implicits/`,
`Y/constructors.rs`, `Y/stats.rs`, `Y/type_args.rs`, `Y/unapply.rs`), **adaptation probes** (§2.15,
`Y/adapt.rs`, `Y/apply*.rs`, `Y/eta.rs`, `Y/named_args.rs`; `typer-compare` per test first), and
the **inliner leftovers** (§2.16) once agreed with #165. Shared hot files: `Y/adapt.rs` and
`Y/apply.rs` stay the adaptation batch's; `Yi/` other than `Yi/type_checks.rs`, `Yi/compiletime.rs`,
`Yi/mod.rs:257` and the macro files is #165's until §2.16 is assigned; `L/` is #165's throughout.
