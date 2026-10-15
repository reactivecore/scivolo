# 19 — Typer groups D, E, G: `{ case … }` and SAMs, self and opaque types, higher kinds

Date: 2026-10-15. Claims are [V] verified by reading the code or running it, [I] inferred. Paths:
`T/` = `ref/scala3/compiler/src/dotty/tools/dotc/typer/` (3.9.0), `C/` = `.../dotc/core/`, `X/` =
`.../dotc/transform/`, `D/` = `.../dotc/ast/`, `Y/` = `crates/typer/src/typer/`, `Yn/` =
`crates/typer/src/namer/`, `L/` = `crates/lower/src/`, `S/` = `crates/tasty/src/`. Measured on
main 3545036f with a fresh `just dotty-lower-report` (809 pass, 598 typer refusals, 25 lowering)
and `just typer-report` (tests/pos: 1217 pass, 1866 unsupported) [V]. `d01`–`d10`, `e01`–`e07`,
`g01`–`g06` name reductions of the tests, each run through `just typer-compare` against dotty's
typed trees (the oracle) [V]. `C/SymDenotations.scala` is an empty file in the checkout, its line
numbers below come from research 14 [I]. Follows [research 14](14-typer-groups.md) §D, §E, §G
and [18](18-typer-group-a.md).

**Recommendation.** The three groups are the first refusal of 115 run tests (D 40, E 41, G 34)
and of 568 pos files (D 103, E 196, G 319 by reason) [V]. Reading every test at its offset:
D is three causes — `{ case … }` against `PartialFunction` (22 tests, the typer's `sam_parts`
sees two abstract methods, §2.1), SAMs of abstract classes and of traits with class parents (6,
§2.2) and SAM types mentioning a method's type parameters (7, §2.3) — plus 6 tests that are not D
at all but overload resolution with lambda arguments (§2.4, group B, ~120 lines for 5 tests, the
best ratio of this note). E is `self =>` aliases and `self: C =>` types (17, §2.5) and opaque
types (24, §2.6), each a small typer change plus a lowering phase dotty also has (`ExplicitSelf`,
`ElimOpaque`). G hangs on one line, `type_lambdas.rs:16`, which refuses every `type F[T]`,
`F[_]` and `CC[X] <: Seq[X]` (26 run, 275 pos); what follows it is the subtyper's lambda rules,
largely present (§2.7). Seven MRs under ~700 lines each, order in §3; D1 and E both need a
lowering half, and E's opaque types want A4's implicit-scope paths (research 18 §2.8).

## 1. The refusals

First refusals of `just dotty-lower-report` and the pos counts of `just typer-report` [V]:

| run | pos | reason | `Y/` site | § |
|---:|---:|---|---|---|
| 29 | 62 | function parameter without a type | `closures.rs:65` | 2.1 (22), 2.4 (6), out (i5257, #39) |
| 10 | 26 | single abstract method type outside the subset | `sams.rs:26,43,45` | 2.2 (4), 2.3 (6) |
| 1 | 15 | parameter type of a wildcard | `closures.rs:59` | out (16405: `inline` match on `(_, _) => R`, #82) |
| 17 | 93 | self type | `classes.rs:51` | 2.5 |
| 24 | 100 | opaque type | `Yn/checks.rs:49` | 2.6 |
| 0 | 3 | implicit scope of an abstract or opaque type with a companion | `implicits/scope.rs:74` | 2.6 |
| 20 | 275 | higher-kinded abstract type | `type_lambdas.rs:16` | 2.7 G1–G3 |
| 3 | 6 | higher-kinded type lambda parameter | `type_lambdas.rs:34,58` | 2.7 G4 |
| 3 | 9 | type lambda parameter with a variance | `type_lambdas.rs:31` | 2.7 G4 |
| 2 | 7 | type lambda other than an alias's right-hand side | `type_lambdas.rs:19` | 2.7 G4 |
| 4 | 6 | wrong number of type arguments | `type_trees.rs:25`, `type_apply.rs:99` | 2.7 (3 are `throws`, A5; transparentArrowAssoc is B) |
| 2 | 16 | class type constructor argument | `type_lambdas.rs:112` | 2.7 G1 |
| 0 | 14 | type constructor without arguments | `type_trees.rs:155` | 2.7 G1 |
| 0 | 3 / 2 | alias of a type constructor; polymorphic method overriding a member | `type_defs.rs:20`; `Yn/inherited.rs:20` | 2.7 |

## 2. Causes

### 2.1 D1: `{ case … }` and lambdas against `PartialFunction` (22 tests)

`val pf: PartialFunction[Int, Int] = { case x if x > 0 => x }` (`d01`), `xs.collect { case … }`
(`d02`, result type a variable), `x => x match { … }` and `_.toString` against a
`PartialFunction` (`d07`) all stop at `closures.rs:65` [V]. Cause [V]: `typed_case_lambda`
(`case_lambdas.rs:16-51`) builds `(x$1) => x$1 match { cases }` and `typed_function`
(`closures.rs:21-35`) asks `function_parts` (no: not a `FunctionN`) then `sam_parts`
(`sams.rs:20-58`); `sam_method` (`:106-113`) wants exactly one abstract method, but
`PartialFunction` has `apply` (from `Function1`) and `isDefinedAt`, so it answers `Ok(None)`, no
formals, and the untyped parameter is refused. dotty's `SAMType.unapply` (`C/Types.scala:6165-6188`)
special-cases `PartialFunctionClass` to `List(PartialFunction_apply)` (`:6169-6176`);
`decomposeProtoFunction` (`T/Typer.scala:1684-1736`, SAM case `:1719-1731`) then yields the
formals, and `typedClosure` (`:2132-2179`) gives the closure the SAM parent as `tpt`
(`:2140-2156`; `PartialFunctionOf` when the parent is underspecified, `:2151-2153`).
Oracle shape (`d01`, `d07`) [V]: `Block(DefDef $anonfun(x$1: Int): Int = x$1 match {…},
Closure(…, TypeTree(PartialFunction[Int, Int])))` — identical to a trait SAM closure; a plain
lambda `x => x match …` and `_.toString` get the same `Closure` with the `PartialFunction` type
tree. The case bodies are `Block([], Typed(x, Int))`, the match carries no `@unchecked`
(`D/Desugar.scala:2004-2008`, `MatchCheck.Exhaustive` at `T/Typer.scala:2196`).

Port, typer (~60 lines): `sam_parts` treats a `PartialFunction` class as having the single
abstract method `apply` (`sams.rs:24`), `proto_arity` (`case_lambdas.rs:55-72`) the same;
`to_sam` (`closures.rs:155-169`) already builds the typed `tpt` for a lambda whose type does not
conform; `adapt` must call it for `PartialFunction` expected types too. `sam_arg_ok`
(`applicability.rs:53-62`) then accepts a case lambda for a `PartialFunction` parameter of
`collect`/`recoverWith` without change [I].

Lowering (~300 lines, the real cost): dotty's `ExpandSAMs.toPartialFunction`
(`X/ExpandSAMs.scala:167-239`) turns the block into
`{ class $anon extends AbstractPartialFunction[A, B] with Serializable { final def isDefinedAt(x: A) =
e(x) match { cases → true; case _ => false }; final def applyOrElse[A1 <: A, B1 >: B](x: A1, default:
A1 => B1): B1 = e(x) match { cases; case _ => default(x) } }; new $anon }` (`:186-238`: the
`partialFunRHS` wrapping of a non-match body `:173-184`, `SubMatch` cases in `isDefinedAt`
`:220-224`, the default case symbol `:208-209`, `.subst`/`.changeOwner` `:212-216`). Today a
closure of a non-function type reaches `L/phases/closures.rs:39-40` and is refused only if the
class has no abstract method; `abstract_methods` (`L/phases/sam.rs:9-37`) returns `apply` for
`PartialFunction`, so a typer that lets pf closures through must be paired with a lowering guard
or the phase (a lambda would be made: wrong `isDefinedAt`). Placement: dotty runs `ExpandSAMs` in
the group after `FirstTransform` and before `PatternMatcher` (`Compiler.scala:63,70,84`), i.e.
between `L/phases/mod.rs:118` and `:136`; the new `TypeDef($anon)` is a local class in a block,
which `local_classes::transform` (`:152`, `L/phases/local_classes.rs:19-47`) lifts and
`local_lift::name_class` names `Outer$$anon$N` [V, how typer-made `new T { … }` classes flow:
`Y/anon_classes.rs:13-48`]; the class must be appended to `classes` before the loops at
`:119-135` [I]. Members: `AbstractPartialFunction[A, B]`, `java.io.Serializable` parents,
flags `Synthetic | Method | Final | Override` (`:195-198`), erased `applyOrElse` and
`isDefinedAt(Object)` bridges from `L/phases/bridges.rs` [I]. `tests/programs/partialfuns`
under `just nir-cmp` for the class name, parents, method order and the default case's
positions (`tree.span`, `:193,198`) [I].

Tests [V by offset]: emptypf, future-flatmap-exec-count, i2396, i2396b, i2396c, i2456, i4177,
i4241, i659, i8662, lift-and-unlift, partialFunctions, pf-sub-cases, single-case-expr, t1220,
t4027, t5300, t6333, t6406-regextract, t7571, value-class-partial-func-depmet,
virtpatmat_partial_backquoted. Next blockers among them [I]: i2396b/c (patterns of classes in
classes, lowering), pf-sub-cases (`sub match`, lowering), i659 and value-class-partial-func-depmet
(#88, a pf in a value class with a dependent `a.C`), t4027 (`for` with `case` generators, F),
future-flatmap-exec-count (futures in the VM). About 14 should pass with D1 alone.

### 2.2 D2: SAMs of abstract classes and traits with class parents (4 tests, plus givens)

`val f: NonJVMSam = x => x + 1` with `abstract class NonJVMSam` (`d05`), `given Conversion[String,
Text] = Text(_)` (`d08`, `Conversion` is an abstract class; the refusal surfaces as "no implicit
argument" at the `summon` because the given's completion fails inside the search [V]),
`trait MyFunction0[+R] extends MyFunction` with `class MyFunction` (`d09`) stop at `sams.rs:26`
(`TRAIT` required) and `:43` (`class_parent`) [V]. dotty accepts them in the typer — the oracle is
the same `Closure` with `TypeTree(NonJVMSam)` (`d05`) — and expands them in `ExpandSAMs`
(`X/ExpandSAMs.scala:58-116`): `isPlatformSam` is `JavaPlatform.isSam`
(`config/JavaPlatform.scala:49-57`: a `NoInits` trait whose superclass is `Object`, directly
inherited traits `NoInits`, no outer needed, no fields, `samExpansionNotNeeded`); anything else
becomes `AnonClass(List(tpe), List(samName -> implSym), refinements)` (`D/tpd.scala:386-412`), the
lambda's `$anonfun` body moving into the class as the SAM method. `SAMType.samClass`
(`C/Types.scala:6133-6163`) adds the typer-side rule: instantiable without arguments
(`:6136-6143`, one applicable constructor, varargs allowed as in i15855's `MyFunction1[R](args: R*)`)
and not final or sealed with `appliedRef <: selfType` (`:6144-6145`).

Port: typer ~30 lines (`sams.rs:25-32` → `samClass`'s rules; `has_no_inits`, `classes.rs:58`,
is at hand); lowering ~200 lines sharing the `$anon` builder of §2.1: the general
`toAnonClass` for closures whose class is not a platform SAM, `L/phases/closures.rs` untouched
since such closures no longer reach it. Scala Native inherits the JVM rule — the plugin runs after
`ExpandSAMs` and only sees closures of platform SAMs [V, `Compiler.scala:70`; no `isSam` in
`ref/scala-native/nscplugin`]. Tests: non-jvm-sam-non-apply, t8280 (then A0's conversion
values), i15855 (with §2.3), convertible (then the 3.9 `into` modifier, out), and the
`given Conversion[…] = lambda` shapes of research 18 §2.1 (`a05`) and
string-context-implicits-with-conversion (§2.6).

### 2.3 D3: SAM types over a method's type parameters or open variables (7 tests)

`def make[A](x: Int): T[A] = a => x` (`d06`), `given [A: Fragable] => Fragable[List[A]] = x => …`
(fragables-extension), `implicit def listEncoder[A](…): JsonObjectEncoder[List[A]] = l => …`
(i10068c), `fromScala[A](…): Ord[A] = (l: A, r: A) => …` (i11676), `given showEither: [A, B] =>
… => Show[Either[A, B]] = _.fold(…)` (i7788), i10068a, i23897 (then #43) all stop at
`sams.rs:43` [V]: `:33-41` refuses any `Type::Sym` of a `TypeParam` owned by a `Def`, and `:25`
any not fully defined type. dotty has neither rule: `typedClosure` requires
`isFullyDefined(samParent, ForceDegree.all)` (`T/Typer.scala:2149`), forcing open variables, and
method type parameters are ordinary types. Port: delete `:33-44`, replace `is_fully_defined(t)`
at `:25` by `fully_defined` with forcing (`closures.rs:57`, same helper); ~30 lines, then probe
`d06` through `just lower-run`: the closure's parameter types erase to the parameters' bounds in
`L/phases/closures.rs:42-50` as any generic lambda does [I].

### 2.4 Not D: overloads with lambda arguments (6 tests, group B)

`Map(1 -> 1).map { case (k, v) => … }` (`d03`) refuses while `List((1, 2)).map { case (k, v) => … }`
(`d04`) types [V]: `Map.map` is overloaded (`map[K2, V2](f: ((K, V)) => (K2, V2))` and
`IterableOps.map[B]`), and the typer types the lambda argument without an expected type while
resolving, so `closures.rs:65` fires. dotty's `resolveOverloaded1` (`T/Applications.scala:2536-2676`)
narrows by `typeShape` (`:2543-2552`: an untyped `Function` becomes `FunctionN[Any, …, shape(body)]`,
a `{ case … }` becomes `PartialFunction[Any, Nothing]`), `narrowByShapes` (`:2651-2657`,
`isFunctionWithUnknownParamType`), then `pretypeArgs` (`:2852`) types the lambdas against the
formals all remaining alternatives agree on. Ours: `Y/alternatives.rs`, `overloads.rs` (187/193
lines) have no shape step. ~120 lines in a new `Y/arg_shapes.rs`. Tests: overloads
(`M3.f("abc", { case s: String => s })`), t2544 (`indexWhere(_ >= 2, 1)`), t3603 (`IntMap.map`),
t4660 (`Map.map`), i25077 (`for (k, v) <- Map(…)` desugars to `Map.map`), iterator-from
(`keyValues map { case (k, v) => … }` at :68, same family [I]). transparentArrowAssoc's "wrong
number of type arguments" at its user `ArrowAssoc` is B too [I].

### 2.5 E1, E2: self aliases and self types (17 tests)

`class A { self => … }` (`e01`), `abstract class TypeHelpers: self: C => def f = x` (`e02`),
`trait B { this: A => def t = foo }` (`e03`) stop at `classes.rs:48-52`, which admits only the
`_: O.type` self of objects [V]. dotty: `Desugar.scala:1099-1103` gives the self `ValDef` the
class type when it has none and the `SelfName` flag; `Namer.completeConstructor`
(`T/Namer.scala:1632-1646`) makes `selfInfo` a term symbol (`createSymbol(self)`, `:1641`) entered
into a scope of its own by `inClassContext` (`C/ContextOps.scala:125-132`), found by name only
from inside the class body (`:31-35`; invisible in the parents' context, `:43`);
`ClassInfo.selfType` (`C/Types.scala:5594-5609`) is `givenSelf & appliedRef` (the given self alone
for modules), and `ThisType.underlying` is that self type (`:3141-3148`), which is what gives
`C.this` the members of the self type. Oracle shapes [V]: the template's self slot is
`ValDef(A.self [PRIVATE LOCAL], InferredTypeTree(A))` (`e01`), `ValDef(TypeHelpers.self, Ident(C))`
(`e02`), `ValDef(B._, Ident(A))` for `this: A =>` (`e03`); a reference `self.x` is
`Select(Ident(self) sym=A :A.this, x)` — an `Ident` named `self` whose symbol is the class and
whose type is `A.this`, also from an inner class; a member of the self type is
`Ident(x) sym=C.x :TypeHelpers.this.x` (`e02`) or `Select(This(B), foo) sym=A.foo` (`e03`).
After the typer `ExplicitSelf` (`X/ExplicitSelf.scala:41-59`, in the group of `ExplicitOuter`,
`Compiler.scala:93-94`) turns an `Ident` typed `C.this` into `This(C)` and casts the qualifier
of a member not owned by a base class to `S & C.this.type` (`:29-39`).

Port, typer (~150 lines): `complete_class` defines the self symbol (`Kind::Val`, flags
`PRIVATE | LOCAL`, info the typed tpt or the class type; not in `decls`) and records the given
self type; `find_ref` (`Y/find_ref.rs:41-87`, `Level::Members(cls)`) answers the self name with an
`Ident` of symbol `cls` and type `This(cls)` only for levels inside the body; `typed_class_def`
(`classes.rs:104`) emits the `ValDef` instead of typing it as a value; `has_undeclared_members`
(`:147-153`) goes. Tasty (~40 lines): `Universe::set_self_type` beside `set_parents`
(`S/symbols/define.rs:89-93`) and `this_type` (`S/types/widen.rs:128-137`) returning
`And(given, applied)` when one is set, so `widen(This(c))` (`:46-49`), `members_within`
(`Y/members.rs:29`, now the class only) and subtyping see the self type's members; library self
types stay undecoded (`S/view.rs:32,60` has no self slot; dotty's rule would apply to them too)
[I]. Lowering (~60 lines): `L/phases/explicit_self.rs` next to `explicit_outer`
(`L/phases/mod.rs:149`): `Ident` typed `This` → `This`, member of a non-base owner →
`This.asInstanceOf[S & C.this.type].m` (`ExplicitSelf.scala:29-39`), erasing through
`erase_and` (`S/erasure/mod.rs:87`). Tests: E1 (`self =>`): i789, t6104 (`case me@Self` stable
pattern), t6206 (`self(this)` apply insertion), i1990b (then inline), typeclass-derivation2d,
CollectionTests (large), anon-mirror-gen-local (#43); E2: 26071, i10905, i8582, t266
(`self: Test.type =>`), t4601, i12032 (then `new Foo(vs) with FooBar`, targetName), i8064 and
supercalls-traits, supercalls-traits-targetname (then `abstract override`, mixin),
i13332intersection (Selectable, out). About 10 should pass; 93 pos files.

### 2.6 E3: opaque types (24 tests)

`object O { opaque type T = Int; def t(n: Int): T = n; extension (x: T) def v: Int = x }` (`e04`),
a top-level `opaque type Nat = Int` with `object Nat` (`e05`), `opaque type Box[A] = List[A]`
(`e06`) and `opaque type Pos <: Int = Int` (`e07`) all stop at `Yn/checks.rs:49-50` [V]. dotty:
`Namer.typeSig` (`T/Namer.scala:1178-1191`) sets the symbol's info to its bounds
(`opaqueToBounds`, `C/SymDenotations.scala:444-474` [I]) and keeps the alias as a refinement of
the owner's self type, read back by `opaqueAlias` (`:1398-1410` [I]) when the prefix is the
owner's `this`; the comparer retries with `liftToThis` (`C/TypeComparer.scala:1611-1619`,
`:1717-1745`: a `TermRef` to a module, a `TypeRef`'s prefix or an applied constructor is
rewritten to `M.this` when `ctx.owner` is inside `M` and `M` is `Opaque`), `typedSelect` heals
member selections on opaque references (`T/Typer.scala:832-857`), and `ElimOpaque`
(`X/ElimOpaque.scala:22-70`, `Compiler.scala:91`) replaces every opaque alias by its right-hand
side before `ExplicitOuter`; erasure never sees one (`C/TypeErasure.scala:356,383,564`).
Oracle shapes [V]: the owner's module class carries `OPAQUE` (`O$ [OBJECT OPAQUE]`, the package
object `e05$package$ [OBJECT SYNTHETIC OPAQUE]`), the type `T [OPAQUE]` keeps its alias tree;
inside `O`, `x + x` on `x: T` resolves `Int.+` and the result of `def t(n: Int): T` is `O$.T`;
outside, `O.t(2)` has type `O.T` (`TypeRef(TermRef(O), T)`) and `.v` is found as an extension of
`O` (`e04`); the signatures erase through the alias (`v#(scala.Int)`,
`first#(1,scala.collection.immutable.List)` for `Box[A]`, `e06`).

Port, typer (~200 lines): `checks.rs:49` goes; completion (`Yn/complete.rs:44-48`) of an
`OPAQUE` type stores `Bounds(lo, hi)` as the info (`typed_type_bounds`'s `alias` form,
`type_defs.rs:52-90`, gives both; default `Nothing..Any`) and the alias through a new
`Universe::set_opaque_alias`; references typed inside the owner get the `This(owner)` prefix as
members do (`this.rs:158-171`), so no lifting is needed for script-internal comparisons; the
implicit scope of `O.T` must include `O`'s members and `T`'s companion (`implicits/scope.rs:74`,
`parts.rs`; dotty `addPath`, research 18 §2.8 — A4), `type_arity` (`type_trees.rs:126-149`) the
alias for HK opaques. Tasty (~80 lines): `referenced_alias` (`S/types/widen.rs:86-98`) and
`meaning` (`S/subtype/refs.rs:157-189`) return the alias when the prefix is the owner's `This`,
the declared bounds otherwise (today `alias_of` refuses `OPAQUE` and a script symbol has no
`opaque_bounds`, `S/symbols/info.rs:46-53`, so it would come out as `Nothing..Any`); `liftToThis`
for values typed outside and used inside: `Cmp` takes the enclosing opaque module from the typer
(`TypeComparer.scala:1717-1745`) [I, needed by i9928/i6716 shapes with companions]. Lowering
(~60 lines): `L/phases/elim_opaque.rs` with `explicit_outer` (`mod.rs:149`) mapping every tree
and symbol type through the aliases, after which erasure, type tests and bridges need no opaque
knowledge; without it `erase_symbol` (`S/erasure/mod.rs:116-119`) would erase an opaque to its
upper bound. Tests: likely clean after E3 — i14587.opaques, i5455 (implicit class, A0 views),
i6716 (top-level, given in the companion), i9928 (givens in companions and `LowPrio`), i18638
(bounded HK opaque shadowing a top-level alias), opaque-immutable-array-xm (HK opaque, ClassTag);
then A5 (`?=>`): ConfManagement, Signals1, Signals2, loops-alt; then #82: i12914, i13461-b,
opaque-inline2, i8577a–i (9, inline `unapplySeq` extensions); option-extract also needs
`(=> B) => B` ("by-name function parameter", `function_types.rs:53`); string-context-
implicits-with-conversion needs §2.2 and A0. 100 pos files.

### 2.7 G: higher-kinded abstract types and type lambdas (34 tests)

Every `type F[T]`, `F[_]`, `CC[X] <: Seq[X]`, `+CC[X] <: GenSet[X]` desugars to a
`LambdaTypeTree` whose body is a `TypeBoundsTree`, refused at `type_lambdas.rs:15-17` (`g01`,
`g02`, `g03`, `g05`, `g06`) [V]. dotty: `typeIndexedLambdaTypeTree` (`T/Typer.scala:2843-2852`),
`Namer.typeSig` with `abstracted` = `HKTypeLambda.boundsFromParams` (`T/Namer.scala:1124-1176`,
`C/Types.scala:4598-4620`: `>: Nothing <: [T] =>> hi`, the lower bound kept `Nothing`), variances
treated as invariant on the upper lambda (`:1163-1172`). Oracle shapes [V]: the abstract member's
tree type is `>: Nothing <: [T >: Nothing <: Any] =>> Any` (`g01`), a class parameter `F[_]`
the same plus the class-level `TypeDef F [PRIVATE LOCAL]` (`g02`), a method parameter
`CC[X] <: Seq[X]` is `>: Nothing <: [X] =>> Seq[X]` and its inferred argument is the class
constructor `List` itself, not eta-expanded (`g03`); `Functor[List]` keeps `List` as the
argument (`g02`); `type F[+A] = List[A]` is `[A] =>> List[A]` with `COVARIANT` on the
parameter symbol only (`g04`). Our `Type::Lambda` (`S/types/mod.rs:73-77`) carries no
variances; the TASTy decoder already represents library `type F[T]` as `Lambda { result: Bounds }`
(`S/subtype/refs.rs:174-181`), which `type_apply.rs:119-137` (`bounds_of`) and `compare_applied2`
(`S/subtype/applied.rs:38-44`, `Meaning::Abstract` applied to the arguments) accept; two lambdas
compare in `S/subtype/third.rs:21-22`, a lambda against a class constructor in `hk.rs:156-174`
(`eta_expand`), variables as constructors in `hk.rs:79-141` (`compareAppliedTypeParamRef`,
`C/TypeComparer.scala:1248-1283`); `liftIfHK` (`:2804-2828`) is `hk.rs:27-43` [V].

- **G1 abstract HK members and class or method parameters** (26 run / 275 pos):
  `type_lambdas.rs:15-20` admit a bounds body (type `Bounds(Nothing, Lambda(params, hi))` as
  dotty) and lambdas whose owner is a type parameter (`Yn/params.rs:123` calls `type_def_info`)
  or a bound; `type_lambdas.rs:104-113` admit a class constructor as the argument of an HK
  parameter (`applied.rs:180` `apply` must eta-expand it on demand, `hk.rs:156`);
  `check_simple_kinded` (`type_trees.rs:153-157`, 14 pos) only in value-type positions
  (`T/Checking.scala:1425`); overriding `type F[T] = List[T]` against `type F[T]` through
  `overrides.rs` (lambda vs. bounds-of-lambda, `third.rs:21`); `Yn/inherited.rs:18-23`
  (polymorphic overriding, t7200b); erasure of `Applied(abstract F, args)` already yields
  `Object` (`S/erasure/mod.rs:79-84`). ~500 typer + 150 tasty [I]. Tests: i22888 (then
  `Array[A[Int]]`'s ClassTag), t7200, t3452f, tcpoly_overriding (then a bridge for `b: t[Int]`),
  i13087, i13986, i13987, i15998, t7126 (`CC[T]` inferred from `Nothing`), tcpoly_monads
  (then implicit conversions), tcpoly_parseridioms (then by-name, D1), i24826 (then §2.2),
  i5527 (then §2.6), i23409, extension-specificity2 (then G4), boehm-berarducci and i13304
  (polymorphic function types, out), context-functions (A5), i13146poly (#43), t2594_tcpoly
  (type projections, out), enrich-gentraversable (`IterableOps[A, Iterable, _]`, then `IsIterable`).
- **G4 type lambdas with variances, HK parameters, in expressions** (8 run / 22 pos):
  `type F[+A] = …` and `[+A] =>> …` (`:30-32`: a flag on the parameter symbol, variance taken
  structurally by `S/symbols/variance.rs` [I]); `type Foo[F[_], T] = ErasedFoo[F[T]]`
  (`:33-35,52-59`; hk-alias-unification, i13332a #43, named-tuples-strawman-2 #39); a lambda
  outside an alias (`:18-20`: `given [T] => Functor[[A] =>> T => A]`, `g05`; implicit-functors'
  `type Const[c] = [t] =>> c` and `F[_[_]]`); `throws` (i13691b, i13961a, saferExceptions: a
  context-function alias, A5); i24056 (then Selectable), safeThrowsStrawman (erased, out). ~400 [I].

## 3. MRs

Each gated by `just gates`, `just typer-compare` on the repros that become golden inputs
(`crates/typer/tests/typed`), `just dotty-lower-report` and `just snapshot-diff programs lower
typer`; the lowering MRs add a `tests/programs` entry under `just nir-cmp`. Sizes are Rust
lines of diff [I]; tests are first refusals from §2.

| MR | scope | lines | tests |
|---|---|---:|---|
| D0 lambda shapes | §2.4 `typeShape`/`narrowByShapes`/`pretypeArgs` in overload resolution | ~120 | overloads, t2544, t3603, t4660, i25077 (+iterator-from, transparentArrowAssoc [I]) |
| D1 partial functions | §2.1 typer (PF as a SAM, `to_sam` for PF) + lowering `expand_sams.rs` `toPartialFunction`, `tests/programs/partialfuns` | ~380 | emptypf, i2396, i2456, i4177, i4241, i8662, lift-and-unlift, partialFunctions, single-case-expr, t1220, t5300, t6333, t7571, virtpatmat_partial_backquoted (+8 after F, #88, lowering gaps) |
| D2 SAM classes | §2.2 `samClass` rules, §2.3 method type parameters; lowering general `AnonClass` expansion | ~300 | non-jvm-sam-non-apply, i15855, i10068a, i10068c, i11676, i7788, fragables-extension; t8280 and `given Conversion = lambda` with A0 |
| E1 self types | §2.5 typer, `set_self_type`, `explicit_self.rs` | ~260 | i789, t6104, t6206, typeclass-derivation2d, 26071, i10905, i8582, t266, t4601, i12032 (+i8064, supercalls-traits×2 after `abstract override`); 93 pos |
| E2 opaque types | §2.6 typer, tasty prefix rule and `liftToThis`, `elim_opaque.rs`, implicit-scope paths (with A4) | ~420 | i14587.opaques, i5455, i6716, i9928, i18638, opaque-immutable-array-xm (+13 after A5, #82, D2); 100 pos |
| G1 HK members and parameters | §2.7 G1 | ~650 | i22888, t7200, t3452f, tcpoly_overriding, i13087, i13986, i13987, i15998, t7126, i23409 (+6 after other groups); 275 pos |
| G2 type lambdas | §2.7 G4 | ~400 | hk-alias-unification, extension-specificity2, i24056, implicit-functors (+3 after A5); 22 pos |

Order: D0 → D1 → E1 → D2 → E2 → G1 → G2, by run tests per line with the pos corpus deciding
between E and G; about 2,500 lines and roughly 55 of the 115 run tests, the rest waiting on
A0/A4/A5 (research 18), #82, #43, #39 and the lowering gaps of research 15 [I]. D1 and D2 share
the `$anon` builder (write it in D1, generalize in D2); E1 and E2 both touch `widen.rs`/`refs.rs`
and the `this`-prefixed references, so E1 first; G1 is the only MR that changes the subtyper's
rules for every applied type and goes alone with `typer-stress`.

Risks: the expanded classes' names and member order must match nscplugin byte for byte
(`just nir-cmp`; `$anon` numbering shared with user anonymous classes, `local_lift::name_class`)
[I]; `this_type` returning an intersection changes member lookup and erasure sites for every
class with a self type (`L/phases/bridges.rs:66`, `mixin.rs:141`, `resolve_super.rs:49,69` call
it; use the given self only where dotty does: `ThisType.underlying`, not `appliedRef`) [V sites];
opaque prefixes without a context owner in `Cmp` are the one design decision of E2 (lift in the
typer vs. an owner on the comparer; dotty does the latter) and deserve an ADR; G1 widens
`compare_applied2` to class constructors as arguments, exercised by every `Functor[List]`-style
type in `tests/pos`.
