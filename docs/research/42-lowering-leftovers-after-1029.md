# 42 — Lowering leftovers in dotty's tests/run and tests/run-macros after 2026-10-29 (#165)

Date: 2026-10-30. Claims are [V] verified by reading the code or running it, [I] inferred. Paths
as in research 39: `L/` = `crates/lower/src/phases/`, `Lc/` = `crates/lower/src/codegen/`,
`Yi/` = `crates/typer/src/inlining/`, `Y/` = `crates/typer/src/typer/`, `Yu/` = `crates/typer/src/unpickle/`,
`M/` = `crates/macros/src/`, `T/` = `ref/scala3/compiler/src/dotty/tools/dotc/transform/`.
Measured on main 3acfb698 (integration-1029 merged) with `just dotty-lower-report`
(`tmp/dotty/3.9.0-0.5.12/lower-results.tsv`) and `just dotty-lower-report "" run-macros` [V]:

| suite | tests | pass | typer (texts) | lowering | wrong | VM crash | frontend crash | VM also under SN | skipped | research 39 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| tests/run | 1711 | **1435** | 66 (42) | **5** | 0 | 0 | 0 | 32 | 173 | 1401 pass, 92 typer, 13 lowering |
| tests/run-macros | 321 | **229** | 79 (53) | **2** | 0 | 1 | 2 | — | 8 | 213 pass, 94 typer, 2 lowering, 1 earlier group |

The totals equal ROADMAP's numbers of 10-29: nothing moved since the integration [V]; research 39's
tsv is not in this worktree, so no row-by-row diff. Of research 39's 13 lowering refusals 10 pass
(i12328a, i13332anonprod, i13332shapeless-b, context-functions, i3539, i13332, poly-kinded-derives,
i2266, i7359, typeclass-derivation2a); typeclass-derivation3 newly reaches the lowering. Method:
`tmp/r42/sites.py` greps each refusal text's site in `crates/`; `tmp/r42/probe/*.scala` reduce each
stop to a few lines for `scivolo-lower`; two refusal messages were widened for one build to print
the symbol and its owner (reverted) [V].

**Recommendation.** Seven stops, five causes, all in `crates/lower`, none shares a file with #168's
MRs. One cause holds three tests: free variables that `local_free`/`lambda_lift` miss — a local
object or method called from a class nested in another local class (enums-thunk, anon-mirror-gen-local),
a captured local object passed as a plain module reference (anon-mirror-gen-local), and a lazy
member's initializer whose anonymous class captures a local (typeclass-derivation3). Three stacked
MRs for one coder (§3): **1** `165-local-captures` (3 tests, ~120 lines), **2** `165-super-poly`
(CollectionTests, ~40), **3** `165-trait-params-lib` (i10880, ~50). i5924b (`@static` in a trait's
companion) and annot-mod-class-add-local-class (a macro-made local class) get their own issues (§4);
the eight inliner leftovers of research 39 §2.16 still stop at the typer and stay with the typer
track. After the three MRs `just dotty-lower-report` shows 2 lowering refusals, both with an issue,
which closes #165's first criterion.

## 1. Attribution

### 1.1 The seven lowering stops

| test | site (message) | source | cause | probe |
|---|---|---|---|---|
| enums-thunk | `L/local_free.rs:154` "classes in local classes calling local methods" | 3:14, `enum E { case A1 }` in a lambda or method | the local enum's object `E` (after `local_lazy` a local accessor `Def E` owned by the method) is referenced from `$anon`, the enum value class nested in the local object `E$` [V widened message: `Outer.thunk.$anonfun.E owned by ... Def, td ... E$`] | `tmp/r42/probe/enumdef.scala` (a local enum in a plain method fails the same way; a local class/object pair with `new E {}` in `mk` passes: `localanon3.scala`) |
| anon-mirror-gen-local | `Lc/expr.rs:81` "a reference to an object of a class without its prefix" | 113:10, `Bar.A.mItem` in `locally1` | the Ident refused is `Foo`, not `Bar`: `Bar$` captures the local object `Foo` (outer of `Foo.Item`, parent of `Bar.A`); `lambda_lift` passes it as a bare module Ident with the span of the use, which codegen cannot route to the accessor `local_lazy` made (`l.local_objects` holds it) [V widened message]. Behind it: `Item`'s mirror class references `Bar.A` from a class nested in local `Foo$`, the first row's cause [I, `localobj4.scala` [V]] | `localobj3.scala` (same message), `localobj4.scala` (first row's message), `localobj5/6.scala` pass |
| typeclass-derivation3 | `Lc/expr.rs:95` "a reference to ev$proxy10" | 70:16, binder `m` of `inline ev match` in `new Eq[T] { def eql … }` from `derived` in `case object Thing derives Eq` | an anonymous class in a **lazy** member's initializer (`derived$Eq`, a lazy given) capturing a local of the initializer: at codegen the proxy is owned by `derived$Eq$lzyINIT1`; `lambda_lift` (pipeline 75) did not capture it although it runs before `lazy_vals` (101). The same with a strict `val` passes [V widened message, `derivedstrict.scala`] | `lazyanon.scala`: `lazy val l = { val x = 1; new AnyRef { override def toString = x.toString } }` fails alike [V] |
| CollectionTests | `L/super_accessors.rs:185` "super calls of polymorphic methods" | 174:23, `super.++(xs)` in `override def ++[B >: A]` of `sealed trait List` | a super call of a polymorphic method inside a trait: the deferred accessor's info is `Type::Poly` and the port refuses it; the same call in a class passes (no accessor) [V] | `superpoly2.scala` fails, `superpoly.scala` (class) passes |
| i5924b | `L/static_members.rs:32` "@static members of an object whose companion is a trait, …" | 8:15, `object Base { @static val x = 10 … }` with `trait Base` | `MoveStatics` for a trait companion: dotty moves methods and non-mutable vals into the trait (`T/MoveStatics.scala:33-37`, `T/CheckStatic.scala:22`); nscplugin emits them as static fields and methods of the trait (`NirGenStat.scala:186`). The port requires a plain class in the unit [V read] | — |
| i10880 (run-macros) | `L/trait_params.rs:61` "parameters of library traits" | Test_2 7:29, inlined `new Ent("something") {}` with `trait Ent(name: String)` from the earlier group | arguments to a classpath trait's constructor: `split_parents` and `param_init` refuse any trait whose symbol has no tree, instead of reading its parameter accessors from TASTy (dotty `T/Mixin.scala:263-312` `traitInits`, `wasOneOf(getter, ParamAccessor)`) [V read] | — |
| annot-mod-class-add-local-class (run-macros) | `Lc/callee.rs:56` "calls of methods outside classes" | Test_2 4:7, `class Foo` whose macro annotation adds `def toString = { class Show { def showMe(x: Foo) … }; new Show().showMe(this) }` | the callee `showMe` was made by `Symbol.newMethod(showCls, …)` inside `Symbol.newClass`'s `decls`; at codegen its owner is not a class (`M/reflect/classes.rs:16-48`: `new_class`, `enter_decls`), so the macro-made local class's member keeps a wrong owner or the class symbol is not the one entered [I] | — |

### 1.2 Research 39 §2.16, the inliner leftovers

Still at the typer, none at the lowering [V tsv]: i12914, i13461-b, loops-alt (`Yi/opaque_proxies.rs`,
"inline method of an object with opaque types"); erased-inline-product-1 (`Yi/reduce/bindings.rs`),
-2 (`Yi/compiletime.rs`); lst (`Y/accessor_defs.rs`); i15968 (`Yu/selections.rs`, "selection of an
unknown member in a library inline body"); expr-map-1 (run-macros, `Yi/retype.rs`, "inline body
with a return or label"). Pass now: typelevel-peano, i23279, i4803c. They are typer work (research
39 §2.16 names the files); #165 does not take them.

### 1.3 The other non-typer stops of run-macros

reflect-isFunctionType crashes in the VM (`AssertionError` in a macro), tasty-interpolation-1 and
xml-interpolation-2 stop with a macro's own `report.error` ("Expected statically known
StringContext"/"SCOps"): the macro track's, as in research 39 [V tsv].

## 2. Mechanics of the causes [V unless marked]

1. **Nested local classes calling local methods** (rows 1–2). `local_free.rs:122-128` records a
   call of a local method from the local class itself (`free.called`) and `lambda_lift` passes the
   callee's proxies; lines 151-156 refuse the same call from a class nested in it. dotty has no
   such case: `Dependencies.markCalled` (`T/Dependencies.scala:170`) records the call for the
   enclosure, `markFree` (`:134`) walks out through intermediate classes, and `LambdaLift.proxies`/
   `addFreeParams` (`T/LambdaLift.scala:63,186`) give each class on the path the proxies. The port:
   record the call as for the direct case and let the nested class reach the proxies through its
   outer chain (`local_outer.rs`, `captures.rs`) or own fields. Local objects count because
   `local_lazy` turns them into local accessor methods (`local_lazy.rs:54-56`).
2. **A captured local object** (row 2). The outer argument `lambda_lift` builds for `Bar$` names
   the module symbol `Foo`; `Lc/expr.rs:77-83` handles static modules and prefixed ones only. Either
   `lambda_lift` emits the accessor call `local_objects[Foo]()` or `gen_ident` does [I which].
3. **Anonymous classes in lazy initializers** (row 3). `local_classes`/`local_free` see the class in
   the lazy val's rhs (owner `Kind::Val`, `is_term` true), yet no proxy is made; `lazy_vals` later
   moves the rhs into `lzyINIT1` and re-owns the local. The coder finds the skipped step with
   `lazyanon.scala` (4 lines); dotty lifts after `LazyVals` (`Compiler.scala:125,132`), so a port
   that treats a lazy member's rhs as a method body in `local_classes` is the likely shape [I].
4. **Polymorphic super accessors** (row 4). dotty keeps the full `superInfo` (`T/SuperAccessors.scala:77-86`)
   and the call site re-applies the type arguments; `ResolveSuper.rebindSuper` (`T/ResolveSuper.scala:85`)
   implements the accessor in the mixing-in class. The port: drop the `Poly` check at
   `super_accessors.rs:184-186`, keep the `TypeApply` around the accessor select, and let
   `resolve_super.rs` forward type arguments (erased anyway) [I ~40 lines].
5. **Trait parameters of classpath traits** (row 6). `trait_params.rs:60-62,80-82` key on
   `symbol.tree.is_some()`; the parameter accessors of a TASTy trait are its getters with the
   `PARAMaccessor` flag, which `implemented_by`/`forwarder` (`trait_fields.rs`) already take by
   symbol. Needed in library code too: any Scala Native or user library trait with parameters [I].

## 3. Batches

Stacked MRs for one Opus coder, disjoint by file, ordered by tests per line; each gated by
`just gates`, `just dotty-lower-report NAME` on its tests, `just snapshot-diff programs lower`
without a worsened row, and one `tests/programs` entry per cause (NIR identical to Scala Native's):

| MR | branch | tests | lines | owns | proves it |
|---|---|---|---:|---|---|
| 1 | `165-local-captures` | enums-thunk, anon-mirror-gen-local, typeclass-derivation3 | ~120 | `L/local_free.rs`, `L/lambda_lift.rs`, `L/local_classes.rs`, `L/captures.rs`, `L/local_outer.rs`, `L/local_lazy.rs` | `tmp/r42/probe/{enumdef,localobj2,localobj3,localobj4,lazyanon,derivedstrict}.scala` lower and run; the 3 tests pass |
| 2 | `165-super-poly` | CollectionTests | ~40 | `L/super_accessors.rs`, `L/resolve_super.rs` | `superpoly2.scala`; `nir-cmp` of a trait with `super.m[B]` against Scala Native |
| 3 | `165-trait-params-lib` | i10880 | ~50 | `L/trait_params.rs`, `L/trait_fields.rs` | a two-unit `tests/programs` case (trait with parameters in one, `new T(…) {}` in the other); `just dotty-lower-report i10880 run-macros` |

MR 1 may need `Lc/expr.rs` (cause 2, ~10 lines) and `L/lazy_vals/` (cause 3); both are free of
#168's files (`Lc/positions.rs`, `method.rs`, `externs.rs`, `annotations.rs`, `erased.rs`,
`L/getters.rs`, `Y/dependent_results.rs`, `select.rs`, `access.rs` are not touched by any batch).
If cause 3 turns out to need a pipeline reorder (lifting after `lazy_vals`, as dotty), it leaves
MR 1 for its own issue: a reorder touches `L/pipeline.rs` and the lazy-value phases together.
Expected after the three: tests/run 1439 pass, 1 lowering refusal (i5924b); run-macros 230, 1
(annot-mod-class-add-local-class).

## 4. Own issues, not batches

1. **`@static` members of a trait's companion** (i5924b): rare, ~40 lines in `static_members.rs`,
   `static_values.rs` and static fields of a trait in `Lc/fields.rs`; the test exercises the
   shape only. Issue under #88 (statics).
2. **Macro-made local classes** (annot-mod-class-add-local-class): the owner of members made in
   `Symbol.newClass`'s `decls` (`M/reflect/classes.rs`); macro track (#40), one test, needs a
   macro-annotation probe first.
3. The inliner leftovers of §1.2 remain the typer track's (research 39 §2.16 and #164).
