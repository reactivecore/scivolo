# 34 — Lowering, value-class and inlining leftovers (ROADMAP "Next" 5–7)

Date: 2026-10-23. Claims are [V] verified by running or reading, [I] inferred. Paths: `T/` =
`ref/scala3/compiler/src/dotty/tools/dotc/transform/`, `Di/` = `.../dotc/inlines/`, `L/` =
`crates/lower/src/phases/`, `G/` = `crates/lower/src/codegen/`, `Yi/` = `crates/typer/src/inlining/`,
`Y/` = `crates/typer/src/typer/`. Measured with `just dotty-lower-report` on main 342a4d47 (release
0.1.2): 1195 of dotty's `tests/run` pass, 15 stop at the lowering, 2 give wrong output, 1 crashes in
the VM, 1 times out in the frontend, 278 stop at the typer, 32 fail in the VM also when compiled by
Scala Native (VM bugs, not lowering ones) [V]. Report under `tmp/dotty/3.9.0-0.5.12/lower-report.md`.

**Recommendation.** The three roadmap items are mostly done: every family of research 15 §1 and every
value-class test of research 13 §3 passes [V]. What is left is a tail of singles. Today: the TASTy
name rendering bug (L4, a crash, ~10 lines), library classes nested in classes (L1, 3 tests, ~60),
inline methods of nested objects (I3, 3 tests, ~40); then I2 and I1 (4 and 7 tests). Close #88, #135
and #163 (§4).

## 1. State of the roadmap items

| item | state [V] |
|---|---|
| 5 lowering gaps (research 15) | reflective calls (i18612-b, -d), local classes using the enclosing instance, inner classes through prefixes, subclasses of inner classes, local traits, ProtectedAccessors, lambdas in inner classes, JDK forwarders (#135), `HoistSuperArgs` for self calls (#163: i3189, i8064): all pass. Left: §2 L1–L3, L6, L7 |
| 6 inlining leftovers (research 12) | outer `this` through `outerSelect` (i15374, i1990b), inline accessors of polymorphic methods (transparentPrivates), REPL inputs calling earlier inline methods: still refused (§3). `letBindUnless` projections: no test reaches `Yi/reduce/projection.rs:97` any more. #90: unchanged |
| 7 value classes (#88) | all 23 tests of research 13 §3 pass, including i24573 (ExpandSAMs), lambda-sam-bridge, mixin-primitive-on-generic-4/5, t5866, vc-equals; left: i15725 (`Y/retained.rs:74`, a retained inline method in a value class), genericValueClass (I2), t13033 (typer, member not found) |

## 2. The 19 tests stopping at or after the lowering

| id | tests | refusal | root cause | lines |
|---|---|---|---|---:|
| L1 | outerPatternMatch, t8611b, transparent | `L/outer_args.rs:34` "library classes nested in classes" | `new x.Inner`, `new Val(nextId) {}`, `new Outer().new Inner` of a class from the classpath (an earlier group, or `Enumeration#Val`): `pass_outer` only knows `l.outers` of the unit's classes, so no outer argument is passed and the constructor signature lacks it | 60 |
| L2 | i8931, i25943 | `L/outer_fields.rs:98` "classes in anonymous classes using the enclosing instance", `L/nested_reads.rs:57` "classes in local classes capturing values" | an anonymous local class in the outer chain gets no outer accessor, so a class nested in it cannot reach the enclosing instance (`new Trait { val t = x }` inside `new Managed[T](x) {…}`); i25943 reads the captured `a2` in a super-constructor call (`class S extends C25943(a2)` in an anonymous class), which `nested_reads` allows in methods only | 100 |
| L3 | implicitFuns, implicitShortcut | `L/context_results.rs:43,75` | `ContextFunctionResults` flattens `def f1(x: Int): Transactional[Int]` in a subclass but not the abstract `def f1(x: Int): Transactional[Int]` of the trait (no closure body); dotty erases both the same and lets `Bridges` eta-expand the difference (`T/Bridges.scala:129-176`); implicitShortcut's `Base_1.foo` is in another group | 150 |
| L4 | transparentAccess (VM crash), i1144 [I] | `no method p.C::D13inline$p$C$$fuEO` | `crates/tasty/src/names.rs:118-125` renders `InlineAccessor(Expanded(p$C, f))` as `inline$p$C$$f`; dotty's `PrefixNameKind.mkString` puts the prefix on the last segment (`qualToString`, `core/NameKinds.scala:95,105`): `p$C$$inline$f`, which is the NIR name [V]. `SuperAccessor` likewise (`super$B$$x` vs `B$$super$x`; `L/resolve_super.rs:76-84` renames around it) | 10 |
| L5 | mixins1 | `G/fields.rs:125` "assignments through setters" | `x = 4` in a class body where `x` is a `var` of a trait of an earlier group: not a field of the unit; dotty types it as `x_=(4)` | 20 |
| L6 | llift | `L/local_traits.rs:43` | `trait T2 { def f3 = x }` nested in local trait `T1`: dotty counts members of local traits as local (`T/Dependencies.scala:72-76`), so `T2` is a local trait too and `f3` takes `x` | 60 |
| L7 | i1140 | `L/trait_fields.rs:119`, `L/library_traits.rs:29` "lazy values in library traits" | `private lazy val x` of a trait from an earlier group; its NIR has `A$$x` as a method only, the implementing class holds the lazy state | 40 |
| L8 | array-erasure (also `tests/repl/unsupported.check`) | `L/type_tests/runtime_tests.rs:61` | `str.isInstanceOf[Array[Array[?]]]`: dotty's `TypeTestsCasts` tests `ScalaRunTime.isArray(x, 2)` | 20 |
| L9 | LazyLists | `G/expr.rs:85` "a reference to xs" at 66:14 | an anonymous class (`new: … xs.isEmpty`) in an extension method captures the extension's leading parameter `xs`, which the capture analysis does not proxy [I] | 30 |
| L10 | i4947b (wrong output) | stack-trace lines of nested inline calls | `Yi/copy.rs:97` copies an `Inlined` node of a substituted argument with `call: None`, so `L/drop_inlined.rs:20` no longer treats it as a position boundary: the inner `Lib.track {` is reported at the outer call's line (9 for 10, 15 for 16) [I] | 15 |
| — | sticky-extmethod (wrong output) | typer | `1.m("xx")` resolves to the top-level `m(x: Int)(y: String)` ("xx") instead of the extension `m(y: Object)` (1) | typer |
| — | curried-mirror | "companions of generic sealed classes" | mirrors, #129 | — |
| — | genericTupleMembers | frontend timeout (60 s) | typer or inliner performance on generic tuples | — |

Not in the table: 32 "fails in the VM" rows (crash or wrong output also under Scala Native, e.g.
classof, i1387, t5680) and the 173 skipped.

### 2.1 L1: library classes nested in classes

dotty: `needsOuterParam(cls)` = not a trait, `needsOuterIfReferenced` and the outer accessor exists
(`T/ExplicitOuter.scala:256-259`); a classpath class got its accessor from `needsOuterAlways` when it
was compiled (`:216-222`: always, unless it has only local instantiation, i.e. is private or local);
the argument is `singleton(fixThis(outerPrefix(tpe)))` (`:349-355, 413-437`). Port in
`L/outer_args.rs::pass_outer`: for `created` without a tree (or of another unit) with
`enclosing_instance`, not private or local and not a trait, insert `(None, outer)` into `l.outers`
on first use, then take the existing path (`instance(l, class, tpe, at)`, which already handles
`x.Inner`, `new Outer().new Inner` and module prefixes); the constructor signature in
`G/` reads `l.outers` as for unit classes. t8611b additionally passes the outer to the superclass
constructor of the anonymous class (`:425`, done for unit parents in research 15 §1.3). Oracle:
`tests/programs` entry with `Outer_1`/`Test_2`-like groups is not possible (one file); compare with
`just nir-cmp` on a probe using `scala.Enumeration#Val` [I].

### 2.2 L2: classes nested in anonymous classes

dotty gives anonymous classes outer accessors like any class (`DropOuterAccessors` removes unused
ones) and `markFree` (`T/Dependencies.scala:134-165`) makes the captured value a proxy field of the
intermediate anonymous class, read through the outer path; in a constructor the path starts at the
`$outer` parameter (`T/LambdaLift.scala:163-175`). i25943 also needs the 3.9 rule that a local
class's constructor inherits the free variables of the super constructor it calls
(`T/Dependencies.scala:244-252`). Port: `L/outer_fields.rs::through` accepts anonymous local classes
once `explicit_outer.rs` defines their accessor (today only the field); `nested_reads.rs:57` allows
constructor reads through `outer_path(…, in_method = false)`. ~100 lines, probe
`tmp/rv/`-style file first [I].

### 2.3 L3: context results needing bridges

`T/ContextFunctionResults.scala` marks every method with a context-function result with
`ContextResultCount` (abstract ones too: `annotateContextResults` counts the *type*, not the closure
nesting, `:30-40`); Erasure then integrates the counts into the method types, and `T/Bridges.scala`
`:129-176` eta-expands a bridge when overriding and overridden differ in count. Ours flattens by the
closure body (`context_result_count`, `L/context_results.rs:95`), so an abstract method keeps its
closure type and the override is refused. Port: count by type as dotty does, flatten abstract
methods too, and extend `L/bridges.rs` with `etaExpand` for a count difference (closure per missing
context clause, `adaptClosure`). The library annotation `ContextResultCount` on `Base_1.foo`
already decodes (`is_annotated`). ~150 lines; risk: SAM closures of context function types in the
bridge [I].

## 3. Inlining leftovers (typer refusals, `tests/run`)

| id | tests | site | dotty | port | lines |
|---|---|---|---|---|---:|
| I1 | i13334, i24357, implicitFuns2, t5665, toplevel-mixed, typeclass-derivation1, typeclass-derivation2b (7) | `Yi/copy.rs:90` "inline body with a local type, class or import" | `TreeTypeMap.transformDefs` re-owns local classes and their members; `Inliner` keeps `Import` trees | `copy.rs`: fresh symbols for a `TypeDef` with a `Template` (class, constructor, members, `This` of the class mapped), local type aliases; `Import` copied as is | 120 |
| I2 | better-fors-map-inlined, genericValueClass, i2895a, i8530-b (4) | `Yi/this_proxy.rs:42` "inline method of a generic class" | `computeThisBindings` (`Di/Inliner.scala:387-432`): the proxy's info is the prefix type, so `C.this.T` becomes `C_this.T` and dereferences to the argument | lift the guard; `bind_class_this` already types the proxy by `qt`; `map_type` must map `Sym(This(C), T)` through `as_seen_from(…, qt, C)` | 60 |
| I3 | i11914, i11914a, i15317 (3) | `Yi/this_proxy.rs:39` "inline method of a nested object" | `:404-407`: a static module's `this` is `ref(sourceModule)`, a nested object's is the call prefix (`pre`) or `outerSelect` | lift the guard; bind `O_this` to the prefix path as for classes (the static case exists in `bind_class_this`) | 40 |
| I4 | i15374 (and i1990b at `G/expr.rs:85`, its `self` alias of an outer trait) (2) | `Yi/outer_this.rs:38` "inline body referring to an outer this" | `outerSelect(prefix, cls, hops, info)` = `Typed(prefix, cls).outerSelect(hops)` (`:394-397,414,420`), a `Select` with an `OuterSelectName` that `ExplicitOuter.transformSelect` turns into accessor calls | a tpd `OuterSelect { qualifier, hops }` node from the inliner, lowered in `L/explicit_outer.rs` through `outer_this` chains (`L/outer_traits.rs:16-23`) | 120 |
| I5 | transparentPrivates (1) | `Y/accessor_defs.rs:41` | `MakeInlineableDirect` (`Di/PrepareInlineable.scala:114-150`): the accessor's info is the accessed member's, type and implicit clauses included | build `clauses` from `Poly`/implicit `Method` types too; the forwarder applies the accessor's type parameters | 40 |
| I6 | none | `Yi/reduce/projection.rs:97` | `letBindUnless(Pure, arg)(Block(trailing, _))` (`Di/InlineReducer.scala:111-119`, `ast/tpd.scala:1428`) | bind `arg` to a fresh `val` when impure, block of the trailing impure arguments | 15 |
| I7 | REPL only: `inline def f(x: Int) = x + 1` then `f(2)` → "call of an inline method defined elsewhere" [V `tmp/l34/repl-inline.txt`] | `Yi/mod.rs:216`: a symbol without a tree is unpickled from TASTy, which REPL inputs lack | — | keep the typed `DefDef` trees of inline methods of earlier inputs (ADR 0012's wrapper objects) in the REPL's shared state and seed `Inlining.bodies`; transparent calls (`Y/transparent.rs:132`) the same way | 40 |
| I8 | none (#90: invalid programs accepted) | — | `T/ForwardDepChecks.scala` (100 lines): per block, the index of each member definition; a reference to a later `val` is an error at the definition | a tree walk in the typer after `typed_block` (`Y/stats.rs:133`), reporting through `report.rs::error_at` | 80 |

## 4. Ranking and issues

By tests unblocked per line [I]: L4 (2/10), I3 (3/40), I2 (4/60), L1 (3/60), I1 (7/120), L5, L8, L10
(1 each, ≤ 20), L2 (2/100), I5 (1/40), L3 (2/150), I4 (2/120), L6, L7, L9 (1 each), I7 (REPL), I6
and I8 (none). I1's seven tests may stop later elsewhere (typeclass derivation, `toplevel-mixed`).

Issues: #88 and #135 (t8933c passes, `7ca4b81d`) and #163 (i3189, i8064 pass, `08a91314`) are done:
close them, #88 with a follow-up for i15725. #108 (lambda in a nested object capturing the outer
twice) and #151 items 1–2 are byte-level; #151 item 3 and #158 wait on the typer. #114 (`super[Q]`
accessors of library traits): i1144 is a plain `super.x` accessor and still fails after L4's rename,
so probe it with L4 before porting `rebindSuper`'s mix filter (`T/ResolveSuper.scala:85-125`).

Each MR: `just gates`, `just dotty-lower-report FILTER` for its tests, `just snapshot-diff programs
lower` without worsened rows, and a `tests/programs` entry where a single file can show the shape
(L2, L5, L6, L8, I2, I3).
