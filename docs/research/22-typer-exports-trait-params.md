# 22 — Typer groups: exports, trait parameters, access, dependent results, local block types, Dynamic

Date: 2026-10-16. Claims are [V] verified by reading the code or running it, [I] inferred. Paths:
`T/` = `ref/scala3/compiler/src/dotty/tools/dotc/typer/` (3.9.0), `C/` = `.../dotc/core/`, `X/` =
`.../dotc/transform/`, `D/` = `.../dotc/ast/`, `Y/` = `crates/typer/src/typer/`, `Yn/` =
`crates/typer/src/namer/`, `L/` = `crates/lower/src/`, `S/` = `crates/tasty/src/`. Measured on
main c969c399 with the `just dotty-lower-report` data of the main checkout (`tmp/dotty/3.9.0-0.5.12`:
878 pass, 531 typer refusals, 23 lowering) [V]; every test of the six groups was run through
`just typer-dump` to find its refusal site, and the reductions `dep01`–`dep03`, `acc01`–`acc02`,
`bt01`–`bt03` (`tmp/r22/`) through the same [V]. Follows [research 14](14-typer-groups.md) §B and
[20](20-member-lookup-adaptation.md) §2.8; the groups were "later" in research 14.

**Recommendation.** The seven reasons are the first refusal of 75 run tests: `Export` 11, `trait
parameters` 8, `qualified private or protected` 11, `inaccessible member` 9, `dependent result
type` 11, `block type with local symbols` 15, `Dynamic` 10 [V]. Five of them are *one line each* in
the typer — a guard that refuses the construct wholesale — and the dotty code behind them is
small: access boundaries (§2.1, `isAccessibleFrom` is 50 lines and the companion rule alone frees
7 tests), dependent method types (§2.2, we already have `safeSubstParams` as `applied_result`;
one refusal is a false positive on `Random.shuffle`), `Dynamic` (§2.5, a 150-line rewrite, no
lowering), trait parameters (§2.4, a typer rule plus `Mixin`'s initializer order in the lowering).
Exports (§2.3) are the one real port, dotty's `exportForwarders` (~330 lines), and local block
types (§2.6) need dotty's `AvoidMap` in place of our top-level approximation. Seven MRs, ~1,600
lines, about 45 of the 75 tests (§3); the rest wait on #88 (value classes), #82 (inline), #43
(mirrors) and research 21.

## 1. The refusals

First refusals of `just dotty-lower-report` and their site [V]:

| run | reason | site | § |
|---:|---|---|---|
| 11 | Export | `Y/dispatch.rs:76-79` (no arm for `Tree::Export`, the variant name becomes the reason); `Yn/mod.rs:63-86` creates no symbol for it | 2.3 |
| 8 | trait parameters | `Y/classes.rs:94-102`; lowering `L/phases/mixin.rs:46-55` | 2.4 |
| 11 | qualified private or protected | `Yn/mod.rs:94-96` (`private_within.is_some()`) | 2.1 |
| 9 | inaccessible member | `Y/access.rs:19-20` (`is_accessible` `:50-67`), `:82` (`unset_local`) | 2.1 |
| 11 | dependent result type | `Y/apply.rs:69-72` (`mentions_params`); `:93-99` refuses dependent parameters next | 2.2 |
| 15 | block type with local symbols | `Y/stats.rs:145-148` (`no_local_refs`, `Y/avoid.rs:55-143`) | 2.6 |
| 10 | Dynamic | `Yn/inheritance.rs:41-42` (a parent `scala.Dynamic`); `Y/not_a_member.rs:16` | 2.5 |

## 2. Causes

### 2.1 Access: qualified modifiers and companions (11 + 9 tests)

`private[a] trait MeterArg` (Meter, MeterCaseClass), `case class Foo private[example] (val i: Int)`
(i14432a/b/c), `protected[A] def y` (t1994), `private[O1] class Base` (t5162), `class CanThrow
private[Exceptions] ()` (config), `@publicInBinary private[foo] object Baz` (i13215),
`private[p1] trait B extends Any` (t8607), `final class TOD private[test2] (…) extends AnyVal`
(valueclasses-constr) all stop at `Yn/mod.rs:94-96` as soon as the definition is indexed [V].
dotty: `Namer.createSymbol` resolves the qualifier with `privateWithinClass` =
`enclosingClassNamed(mods.privateWithin)` (`T/Namer.scala:221-222`) and stores it as the symbol's
`privateWithin`; a qualified private carries *no* `Private` flag, a qualified protected keeps
`Protected`. Access is `isAccessibleFrom` (`C/SymDenotations.scala:932-983`): `boundary =
accessBoundary(owner)` (`:1520-1525`: `Private` → owner, `privateWithin` if set, `Protected` →
the base, else root), then `accessWithin(boundary)` = `ctx.owner.isContainedIn(boundary)`
(`:937-939`, `:740`) *or* `accessWithinLinked(boundary)` = inside the companion (`:942-945`,
`linkedClass` `:1387`), with the `Local` rules (`isAccessPrivilegedThisType` `:915`, `canBeLocal`
`:2729`), or `Protected` and (`superAccess` or a `ThisType` prefix or `isProtectedAccessOK`
`:948-957`: `enclosingSubClass` `:1395` with `pre.derivesFrom(cls)`, or a constructor in a
super call, or a module owner). Our `is_accessible` (`Y/access.rs:50-67`) knows `private_within`
only for package-level privates (`Yn/mod.rs:104-123`, `crates/typer/src/symbols.rs:43`) and walks
`ctx.owner`'s owners alone: no companion, no protected-through-subclass (`Y/select.rs:111-116`
refuses "protected member through a path" separately).

The nine "inaccessible member" tests are that companion rule [V by site, I by cause]: i14432
(`case class Foo private (val i: Int)`: the synthesized `apply` in the companion calls the private
constructor; our desugar already copies the constructor's `private_within` to `apply`/`copy`,
`crates/syntax/src/desugar/case_class.rs:148`, `class_companions.rs:14`, as `D/Desugar.scala:952,
1008` do), quoted-sematics-1 (`new Name(base, i)` in the companion), t2127 (`object Test extends
Bar(new Test(_))`, the companion's parent call), t8002 (`class A private () { private def x; A.y }`
both directions), tuple-patterns (`new TupleXXL1(elems.clone)` in `apply`),
typeclass-derivation2a (`new ArrayProduct(elems)`; then #43), i4754 (private companion members
in an `inline def`; then "non-public member in an inline method", `Y/inline_refs.rs`, #82).
Two are not: i23477, `class C private (x: Int, y: Int = 1): def this() = this(1)` — the reduction
`acc02` without a companion refuses the self-constructor call itself [V], so the prefix the
`this(1)` path passes to `check_reference` is not one `is_privileged_prefix` (`:103-109`) knows
[I, probe first]; i7375, `Foo("foo").name()` with `private val name` and `extension (f: Foo) def
name()` — dotty's `accessibleType` returns `NoType` for the inaccessible member
(`T/TypeAssigner.scala:84-104`) and `typedSelectWithAdapt` (`T/Typer.scala:799`) falls through to
`tryExtensionOrConversion` (`:4307`); ours chooses the member and refuses at `select.rs:94-96`.

Port (~120 lines): `Yn/mod.rs:94` resolves the qualifier to the enclosing class or package of that
name (the levels of `ctx` hold them) into `syms.private_within`, drops `PRIVATE` for a qualified
private (as `:104-107` does for package privates) and keeps `PROTECTED`; `is_accessible` becomes
`isAccessibleFrom`: `access_boundary`, `access_within`, the linked class (`u.module_class` and the companion lookup
the constructor proxies use [I]), protected through a subclass of the
owner with `derives_from` on the prefix's class (then `select.rs:111-116` goes); the TASTy side
already decodes `PRIVATEqualified`/`PROTECTEDqualified` (`S/symbols/symbol.rs:39-53`). Lowering:
none — nscplugin emits qualified privates as public methods, `L/phases/protected_accessors.rs:28`,
`casts.rs:164` and `codegen/statics.rs:84,97` already treat the qualified flags like the plain
ones [V sites, I for nscplugin]. Tests: config, i13215, i14432a/b/c, t1994, t5162 (then Meter,
MeterCaseClass, t8607, valueclasses-constr with #88, research 20 N1 for Meter's packages);
i14432, quoted-sematics-1, t2127, t8002, tuple-patterns, plus i7375 with the 20-line extension
fallback in `select.rs` and i23477 after the probe.

### 2.2 Dependent method types (11 tests)

`addString(b: StringBuilder, sep: String): b.type` (t5656, `dep01` [V]), `def lazyDep(x: X)(y:
=> x.T)` (t6443-by-name, t6443-varargs, t6443b; `dep03` [V]), `extension (using ctx: Ctx)(tpe:
String)(using env: Env) def :#:(trm)(ext: env.Extra)` (i11583), `val y = g` with `def g(x:
Int)(using d: D)(y: d.T): d.T` (given-eta, eta-expansion of a dependent method), `x4(0)` =
`Tuple.apply(n: Int): Elem[This, n.type]` (tuples1), `xs.runtimeChecked: x.type @RuntimeChecked`
(runtimeChecked, runtimeChecked-2), `mirror.fromProduct(product): mirror.MirroredMonoType`
with `mirror` an implicit parameter (i12919) stop at `Y/apply.rs:69-72` [V]. So does
`util.Random.shuffle(alphaList)` (distinct, `dep02` [V]) whose result `C` is a *type* parameter:
`mentions_params` (`Y/dependent_params.rs:73-86`) answers true for it — a false positive to find
first [I: the clause binder of the implicit clause or `Type::Sym` params of the poly method].

dotty: `assignType(Apply)` (`T/TypeAssigner.scala:302-329`) substitutes the argument types for the
parameter references of a result-dependent method with `safeSubstParams` (`:294-300`;
`safeSubstParam` skolemizes an unstable argument, `SkolemType(argType.widen)`, `:285-287`, the
skolems kept in `SkolemizedArgs` `:327`, `T/Inferencing.scala:861`); parameter-dependent clauses
substitute each typed argument into the remaining formals while typing (`T/Applications.scala:834-845`,
`looksParamDependent`, `safeSubstParam`); `isResultDependent`/`isParamDependent` are
`C/Types.scala:4178-4190`. Ours has `applied_result` (`Y/dependent_params.rs:37-69`) — exactly
`safeSubstParams`, with `fresh_skolem` — used for using clauses (`Y/implicits/clauses.rs:48`)
and default getters (`Y/default_args.rs:75`) but not for the application itself.

Port (~150 lines): D0 the false positive (~10); D1 `apply.rs:69-72` goes, the result after
`typed_aligned_args` (`:111-119`) is `applied_result(result, params, args)` and
`constrain_applied_result` (`Y/adapt_by_result.rs:38`) takes that; the eta-expander
(`Y/eta.rs:132`) builds the closure with the substituted result (given-eta); D2 `:93-99`:
substitute the typed arguments of the clause into the later formals before `typed_aligned_args`,
by-name kept through `:88-92`. Tests: t5656, distinct, given-eta, t6443-by-name, t6443-varargs,
t6443b, i11583 (7); tuples1 then needs research 21 M2, runtimeChecked×2 the annotated type and
#82, i12919 #43.

### 2.3 Exports (11 tests)

Every `export` stops at `Y/dispatch.rs:76-79` when the class body is typed; the namer skips the
clause (`Yn/mod.rs:85`), so the class has no forwarders and `has_undeclared_members`
(`Y/classes.rs:164-170`, used by `Y/did_you_mean.rs:121`) only keeps "not a member" errors quiet
[V]. dotty's `Namer.exportForwarders` (`T/Namer.scala:1236-1561`), run by `processExports`
(`:1566-1618`) at the end of the class completer after `addConstructorProxies` (`:1843-1845`):

- the path is typed ahead (`typedAheadExpr(expr, AnySelectionProto)` `:1248`, `checkLegalExportPath`
  `T/Checking.scala:1211`); selectors as for imports (`importBound` `:848`, `addForwarders`
  `:1490-1498`: a named selector → `addForwardersNamed` `:1450-1460` for the term *and* the type of
  that name, a rename is the forwarder's name; a wildcard → `addWildcardForwarders` `:1467-1488`:
  members without `Private | Synthetic`, no super accessors, no case-class synthesized members of a
  case-class path (`:1469-1471,1478`), no default getters (`:1479`), `given` members only through
  `given` selectors with their bound `:1483-1485`);
- `canForward` (`:1255-1296`): accessible from the path's type; constructors, module classes,
  bridges skipped (`:1270-1271`); not when the class already has the member through inheritance
  (`:1277`, i7086's `b.foo == 1`), when the alias is taken in the same clause, or when it
  overrides a concrete inherited member (`:1285-1293`);
- `addForwarder` (`:1318-1448`): a type member becomes `final type A = path.A` with
  `Exported | Final` (`:1339-1354`, eta-expanded if it takes parameters); a term becomes a method
  with `MandatoryExportTermFlags = Exported | Method | Final` plus the member's
  `RetainedExportTermFlags` (`Infix | Given | Implicit | Inline | Transparent | HasDefaultParams |
  NoDefaultParams | ExtensionMethod`, `C/Flags.scala:543-551`), info `mbr.info.ensureMethodic`
  (`:1392`) — or `StableRealizable` with an `ExprType` for a public stable member on a stable path
  (`:1359-1365`), the right-hand side `path.select(sym).appliedToArgss(params)` with varargs
  re-wrapped (`:1320-1333`, `:1417-1432`), the body of an inline forwarder made inlineable at once
  (`:1423-1429`); default getters are forwarded with their method, renamed with it (`:1298-1313`,
  `:1434-1436`, i19587's `fooWithDefaults$default$1`); parameter flags and annotations copied
  (`:1402-1408`, `:1441-1447`);
- `avoidClashes` (`:1505-1554`) drops the forwarder overloading resolution would not pick when
  two erase alike;
- `typedStats` emits the typed `Export` statement followed by its forwarders
  (`T/Typer.scala:4095-4099`, the `ExportForwarders` attachment `:61`).

Exports in extension clauses (`export cm.*` inside `extension (x: Int)`, export-in-extension) take
the `pathMethod` route: the path is the companion extension method (`exportPathSym` `:1575-1589`),
the forwarder's info is wrapped in the extension's clauses (`addPathMethodParams` `:1367-1393`) and
gets `ExtensionMethod` (`:1396`).

Lowering: nothing export-specific — a forwarder is a `final def` whose body is a selection on the
path, mangled from its own erased signature like every method (nscplugin has no `Exported` case
[I: no reference checkout here; dotty's `Exported` flag is pickled only]); a stable forwarder is a
nullary method (`ExprType`), which `L/` already lowers for parameterless defs [I]; an inline
forwarder is an inline method (#82). Our flags carry `EXPORTED` on both sides
(`crates/syntax/src/ast/flags.rs:64`, `S/tags.rs:41`, `crates/typer/src/symbols.rs:167`).

Port (~400 lines): `Yn/exports.rs` (`exportForwarders`: path typed ahead in the class context,
`canForward`, term/type/stable forwarders as `syms.define` with flags and infos, default getters,
wildcard and given selectors with `import_tree.rs`'s bounds, `avoidClashes` with
`u.erased_method`) called at the end of `complete_class` (`Y/classes.rs:33-64`); the forwarder
bodies are built as typed trees in `typed_template_stats` (`:136-160`) where the `Export`
statement stands — dotty prints the `export` itself in the typed tree, so `just typer-compare`'s
oracle diff must skip it or `tpd` gets an `Export` node the lowering drops [I];
`has_undeclared_members` loses its export half. Then E2 extension exports (~80). Tests: exports
(given and renamed selectors, an extension exported), i7086, i10857 (enum case values and the
synthetic companion as stable forwarders), i13228 (case-class path), i13490.min (a type
forwarder with a stable one, `import MyApi.MyEnum.Foo` through them), i19587 (defaults, `as`,
`infix`) — 6; i14020 (default getters work, `inline def greeting` needs #82), i10724 (inline
varargs forwarders, #82), export-in-extension (E2), export-anyval (#88), i13332intersection
(#43, out).

### 2.4 Trait parameters (8 tests)

`trait Foo(val s: String)` (i1263, i13862), `trait T(x: Int, val y: Int)` (traitParams),
`trait Managed[T](x: T)` (i8931), `trait T(a: Any)` with `trait U(a: Any) extends T` and
`new T(Trace("ta")) with U(Trace("ua")) {}` (traitParamInit), `trait Foo[T: ClassTag]()` (i2795),
`trait FooLaws[A](using Foo[A])` (i7613), `trait B[T: A]` (i11966) stop at `Y/classes.rs:100-102`,
which refuses any trait whose constructor has a value clause [V]; the lowering refuses the same at
`L/phases/mixin.rs:46-55` [V].

dotty, typer: a trait parent given as a type gets a constructor call when it takes parameters and
the superclass does not already extend it (`ensureConstrCall`, `T/Typer.scala:3566-3574`:
`psym.primaryConstructor.info.takesParams && !cls.superClass.isSubClass(psym)`; implicit
parameters are inferred there, `:3303`); `typedParent` checks the trait constructor is accessible
(`:3307-3309`) and `checkParentCall` (`T/Checking.scala:1399-1408`) that a trait never calls a
parent constructor and a class does not call one its superclass already implements;
`RefChecks` adds `checkParameterizedTraitsOK` (`T/RefChecks.scala:960-980`: the base types of a
parameterized base seen from the class and from the superclass agree) and forbids a trait
parameter as a parent prefix (`:188`). Param accessors of a trait are ordinary members until
`Mixin`. Ours: `Y/parent_trees.rs:86-91` returns a trait parent untouched (the rule above goes
there), `super_call_ctx` (`:59-73`) already sees param accessors, `Yn/params.rs` enters them;
`has_no_inits` (`Y/no_inits.rs:28`) is right (a parameterized trait has inits); anonymous classes
with argument-taking parents go through `Y/anon_classes.rs` (traitParamInit).

dotty, lowering (`X/Mixin.scala`): `transformSym` (`:140-156`) makes every accessor of a trait
deferred, drops `ParamAccessor`, expands private names (`:147`) and turns the trait constructor
into `$init$` without parameters (`:153-156`, `:356`); `transformConstructor` (`:224-245`) splits
a parent call into the super call and, for a trait, its arguments as *initializers* (a `Block`
around the call — named or default arguments — has its statements lifted to class fields,
`:229-240`); `traitInits` (`:263-316`) gives the class, for each getter of the mixin it implements,
a forwarder whose body is the next argument for a param accessor (`:287-288`; an overridden
param accessor evaluates and drops it, `:312-314`), `super.x` for a lazy val, `new M(this)` for
an object, `_` otherwise; the template body is `superCallOpt(superCls) ::: (prefix :::
traitInits(mixin) ::: superCallOpt(mixin) ::: setters ::: mixinForwarders)* ::: body`
(`:354-376`). The trait's own `$init$` refers to its parameters through the deferred getters.
Ours: `trait_fields.rs::mixed_in` (`:107-145`) gives getters without body (`:125`) and the
`$init$()` call (`:129-137`); `trait_init.rs:63-68` keeps the constructor's `paramss`;
`constructors.rs` builds the class constructor from the parents' calls; `hoist_super_args.rs`
hoists lambdas in parent arguments; `accessor_names.rs:24-33` has the expanded private names.

Port (~350 lines): typer ~120 (`classes.rs:100-102` go; `ensureConstrCall`'s trait rule and
`checkParentCall`; `checkParameterizedTraitsOK` as a check in `Y/overrides.rs` or `Yn/inheritance.rs`;
a probe that `anon_classes.rs` types `new T(a) with U(b) {}`); lowering ~230 (`mixin.rs:46-55`
go; `trait_init.rs` strips the value clauses of `$init$`; `mixed_in` takes the parent call's
arguments: a param-accessor getter's forwarder gets the argument as its body — `memoize`/
`constructors.rs` then make it a field initialized before `$init$()` — a `Block` parent call lifts
its statements; order as `:354-376`; a `tests/programs/traitparams` under `just nir-cmp` for
field order, the `$init$` signature and the argument evaluation order of traitParamInit).
Tests: all 8 [I: no second blocker found by reading; i2795/i7613/i11966 pass their evidence
through the inferred constructor call].

### 2.5 Dynamic (10 tests, #42)

All ten stop at `Yn/inheritance.rs:41-42`, the class's own `extends Dynamic` [V]: dynamicDynamicTests
(select, apply, named arguments, update, `foo()`, varargs `*`, type arguments), applydynamic_sip,
t5733 (`object A extends Dynamic`), t6353, t6663, t6355 (`applyDynamic(method: String): B` then
`.apply`), i4073, i4600 (`(implicit s: String = "World")` after the arguments), t5040 (abstract
`applyDynamic(m: String)()`), dynamic-anyval (`extends AnyVal with Dynamic`, implicit classes).

dotty rewrites at the use site (`T/Dynamic.scala`): `typedSelectWithAdapt` tries `tryDynamic`
(`T/Typer.scala:930-933`) when the qualifier derives from `Dynamic`, the name is a term and the
tree is not already an expansion (`isDynamicExpansion` `Dynamic.scala:31-39`); with a function or
assignment prototype it assigns `TryDynamicCallType` (`:921-926`, `C/Types.scala:6004`) so that
`typedApply` turns the whole application into `typedDynamicApply` (`T/Applications.scala:1440-1444`;
type arguments `:1624-1625`): `foo.bar(a, b)` → `foo.applyDynamic("bar")(a, b)`, named arguments →
`applyDynamicNamed("bar")(("x", a), ("", b))` (`Dynamic.scala:81-116`), an inserted `apply`
(`foo(1)`) → `applyDynamic("apply")` (`:99-105`); otherwise `typedDynamicSelect` → `foo.selectDynamic("bar")`
(`:125-126`); `typedAssign` → `updateDynamic("bar")(rhs)` (`T/Typer.scala:1545`,
`Dynamic.scala:131-145`); `foo.bar(k) = v` goes through the ordinary `.update` rewrite of
`typedAssign` on the `selectDynamic` result (`:122-123`). The core is `coreDynamic` (`:147-153`):
`Apply(Select(qual, dynName)[targs], Literal(name))`, then typed as any application, so
overloads, varargs, implicit clauses and defaults (i4600) need nothing new. `handleStructural`
(`:184-284`) is `Selectable`, research 20 §2.8, out.

Lowering: none — the expansion is ordinary calls of user methods; `scala.Dynamic` is a `NoInits`
marker trait the mixin phase ignores (`L/phases/mixin.rs:71-76`); `L/codegen/callee.rs:34-38`
refuses only `scala.reflect.Selectable`'s reflective methods [V]. dynamic-anyval needs #88 and the
implicit-class views of research 18 A0.

Port (~150 lines): `inheritance.rs:41-42` go; a new `Y/dynamic.rs`: in `typed_select`
(`Y/select.rs:70-83`, before `extension_visible`) when the qualifier's class derives from
`scala.Dynamic` build the untyped `coreDynamic` tree with a splice for the typed qualifier (as
`parent_trees.rs:92-107` does) and type it with `typed_apply` for `Pt::Any`; for `Pt::Fun`
return a marker the caller `typed_apply` (`Y/apply.rs:10`) recognizes to rebuild `applyDynamic`/
`applyDynamicNamed` with `Y/named_args.rs`'s names and `Y/type_apply.rs`'s type arguments;
`control.rs:83 typed_assign` → `updateDynamic`; the inserted-apply case in `Y/apply_insertion.rs`;
`isDynamicExpansion`'s guard against re-expanding `selectDynamic` itself. Golden inputs in
`crates/typer/tests/typed` from dynamicDynamicTests' lines through `just typer-compare` (spans:
`coreDynamic` puts the selection span on `Select`, the literal is synthetic). Tests: 9, then
dynamic-anyval (#88).

### 2.6 Block types with local symbols (15 tests)

The block's result type mentions a local symbol and `no_local_refs` (`Y/avoid.rs:55-143`) finds
no replacement [V]. Three shapes [V by line, I by cause]: an anonymous class as the result —
HLists, HLists-nonvariant (`new At[…] { type Out = …; def apply }`), byname-implicits-17 (`new
Generic[List[T]] { type Repr = … }`), eff-dependent (`bt03` [V]), interleaving (`new Key { type
Value = Some[Int] }`), t5629b, tcpoly_monads, i13703 (`new Foo { var i: Int = 0 }`), i14340,
enum-Tree (an enum case with two parents, desugared to an anonymous class value), selectable-new;
a local value or object inside another type — valueclasses-nested-object (`() => X.type`, `bt01`
[V]), unambiref (`a.B()` of a local `a`, `bt02` [V]), t7242 (`c.CrashTestTable`, a type member
of a local value); and i3018 (`spaces.map(simplify(_))` with a default argument, to probe).

dotty: `typedBlock` → `ensureNoLocalRefs` (`T/Typer.scala:1555-1577`, `:1594-1623`): if
`escapingRefs` finds a local symbol in the type, `TypeOps.avoid(tp, locals)` (`C/TypeOps.scala:565-584`)
computes a type without them and the block gets a `Typed` ascription (or the expected type when
fully defined and the avoided type does not conform, `:1615-1616`). `avoid` is an `AvoidMap`
(`:462-508`), an `ApproximatingTypeMap` (`C/Types.scala:6588`, `range(lo, hi)` `:6590`) that
maps the *whole* type with variance: a local `TermRef` → `range(Nothing, widened info)`
(`:477-481`), a local `TypeRef` → its alias, or `range(lo, hi)` of its bounds, or for a local
class `range(Nothing, classBound(info))` (`:482-492`); `classBound` (`:422-459`) is the parents'
intersection refined by each non-private declaration whose type is more specific than the
inherited one — or any member when the parent is a `Selectable` (`:436-447`) — closed over `this`
with a `RecType` (`:450-452`); `derivedSelect` keeps `C#T` for members of an avoided prefix
(`:510-517`). The same map serves lifted definitions (`avoidingType`,
`T/TypeAssigner.scala:46-47`; our `wrap_defs`, `avoid.rs:25-49`).

Ours only looks at the top of the type (`avoid.rs:82-125`): a local class → the glb of its parents
when at most two are proper and no refinement is needed (`:84-94`, `needs_refinement` refuses),
a bounded type → its upper bound, a local value → its info, repeated 16 times; a function,
applied or intersection type containing a local is never mapped (`bt01` [V]); prefixes are
dealiased only (`:67-79`).

Port (~300 lines): `Y/avoid.rs` becomes dotty's map — a variance-aware traversal (`u.map_type`
has no variance; `S/symbols/variance.rs` gives class parameter variances) producing lower/upper
approximations: `TermRef` → widen (upper) or `Nothing` (lower), local class → `class_bound`
(parents glb + `Type::Refined` for the more specific declarations, research 20 N4's
representation; `this` references refused instead of a `RecType` [I: an ADR]), a member of an
avoided prefix → the member on the widened prefix (`bt02`: `A#B` as `Sym(Some(A), B)`), type
members → alias or bounds; `stats.rs:145-148` keeps the refusal for what remains. Tests: HLists,
HLists-nonvariant, byname-implicits-17, eff-dependent, interleaving, t5629b, tcpoly_monads (then
research 19 G1), unambiref, t7242 — about 8; i13703, i14340, enum-Tree, i3018 to probe;
selectable-new (reflective calls, out), valueclasses-nested-object (#88).

## 3. MRs

Each gated by `just gates`, `just typer-compare` on the reductions that become golden inputs
(`crates/typer/tests/typed`), `just dotty-lower-report` and `just snapshot-diff programs lower
typer`; the lowering halves add a `tests/programs` entry under `just nir-cmp`. Sizes are Rust
lines of diff [I]; tests are first refusals from §2.

| MR | scope | lines | tests |
|---|---|---:|---|
| X1 access | §2.1 qualifiers in `Yn/mod.rs`, `isAccessibleFrom` in `Y/access.rs` (boundary, linked class, protected via subclass), extension fallback in `select.rs` | ~140 | config, i13215, i14432a, i14432b, i14432c, t1994, t5162, i14432, quoted-sematics-1, t2127, t8002, tuple-patterns, i7375 (+i23477 after a probe; Meter, MeterCaseClass, t8607, valueclasses-constr with #88; i4754 #82; typeclass-derivation2a #43) |
| D dependent | §2.2 D0 false positive, D1 `applied_result` for applications and eta, D2 param-dependent clauses | ~150 | t5656, distinct, given-eta, t6443-by-name, t6443-varargs, t6443b, i11583 (+tuples1 after research 21 M2; runtimeChecked×2 #82; i12919 #43) |
| Y Dynamic | §2.5 `Y/dynamic.rs`, select/apply/assign hooks, parent check | ~150 | dynamicDynamicTests, applydynamic_sip, t5733, t6353, t6663, t6355, i4073, i4600, t5040 (+dynamic-anyval #88) |
| T trait params | §2.4 typer rule + `Mixin` initializers, `tests/programs/traitparams` | ~350 | i1263, i13862, traitParams, i8931, traitParamInit, i2795, i7613, i11966 |
| E1 exports | §2.3 `Yn/exports.rs`, forwarders in the typed template, oracle diff | ~400 | exports, i7086, i10857, i13228, i13490.min, i19587 (+i14020, i10724 after #82; export-anyval #88; i13332intersection #43) |
| E2 extension exports | §2.3 `pathMethod` route | ~80 | export-in-extension |
| B avoid | §2.6 `AvoidMap` with `class_bound` and refinements | ~300 | HLists, HLists-nonvariant, byname-implicits-17, eff-dependent, interleaving, t5629b, unambiref, t7242 (+tcpoly_monads after G1; i13703, i14340, enum-Tree, i3018 to probe) |

Order: X1 → D → Y → T → E1 → E2 → B, by run tests per line; about 1,570 lines for ~45 of the 75
tests, the rest waiting on #88 (8), #82 (5), #43 (4) and research 21 (1) [I]. X1 and D are
independent one-day MRs; Y goes after D because both edit `apply.rs`; T's typer half is small and
can go first if the lowering half waits for #34.

Conflicts with ongoing work (files, from the research 20/21 plans and the merged branches) [V
for the file lists, I for the overlap]:

- Research 21 M2–M4 (match types): `S/subtype/`, `S/erasure/mod.rs`, `Y/type_trees.rs`,
  `Y/seen_from.rs`, `crates/typer/src/unpickle/admitted.rs`, `crates/typer/src/inlining/`. None of
  the MRs here touch them if B keeps its map in `Y/avoid.rs` rather than `S/types/`; D's result
  substitution does not reduce match types (tuples1 waits).
- #34 lowering gaps: `L/phases/constructors.rs`, `getters.rs`, `memoize.rs`, `explicit_outer.rs`,
  `lifted.rs`, `patmat/`, `type_tests/`. T's lowering half edits `mixin.rs`, `trait_fields.rs`,
  `trait_init.rs` and needs the parent call's arguments from `constructors.rs` — land #34's
  constructor changes first or pass the arguments through `trait_fields.rs::mixed_in` only.
- Research 20 leftovers: N1 (`Yn/mod.rs` `index`) overlaps X1 (`Yn/mod.rs:94`) and E1 (`Yn/`, the
  class completer); N2/N5 (`Y/apply.rs`, `applicability.rs`, `overloads.rs`) overlap D and Y
  (`apply.rs`); §2.8 Selectable overlaps Y (`select.rs`) and B (`class_bound`'s Selectable
  refinements) — one coder for the `select.rs` hooks.
- The `83-typer-*` branches edit `Y/select.rs` and `Y/implicits/`; Y and X1's extension fallback
  should start after they merge.

Risks: X1 drops `PRIVATE` on qualified privates, which changes `LOCAL` and the lowering's private
name expansion (`accessor_names.rs:24-33`) for `private[C]` trait members — check `just nir-cmp`
on a trait with one; D's skolems must not leak into inferred types that reach TASTy-independent
golden trees (dotty prints them as the widened type); E1's forwarders are typed trees without an
untyped source, so every pass that maps `by_tree`/`def_trees` (`Yn/mod.rs:129-132`) needs a
`None` arm, and the `export` statement in dotty's typed tree is the one place `typer-compare`
must ignore; T changes the evaluation order of parent arguments for *every* class with a
parameterized trait and nscplugin is the oracle (`traitParamInit`'s trace); B replaces a
conservative approximation by a map that touches every block type — `just typer-stress` and the
pos corpus before merging.
