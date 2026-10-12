# 15 — Lowering gaps and wiring (ROADMAP items 4 and 5)

Date: 2026-10-12. Claims are [V] verified by reading the code or running it, [I] inferred. Paths:
`T/` = `ref/scala3/compiler/src/dotty/tools/dotc/transform/` (3.9.0), `L/` =
`crates/lower/src/phases/`, `G/` = `crates/lower/src/codegen/`, `S/` = `crates/scalai/src/`. Measured
on the #88 branch (`tmp/wt/88c`, main's inlining-free base: 412 of dotty's `tests/run` pass, 34 are
refused by the lowering) and on the inlining stack (`tmp/wt/82d`: 576 pass, 64 refused, 22 of them
"members value classes inherit", which #88 lifts) [V]. Probes under `tmp/rv/p1..p11.scala`.

**Recommendation.** Item 4 is four lowering MRs, each lifting one family of refusals: outer instances
of local classes (with inner-class creation through prefixes), subclasses of inner classes,
ProtectedAccessors, local traits and lambdas in inner classes; then the small refusals and the byte
issues. Item 5 is two small MRs: `scalai FILE.scala` through `lower::compile` with scala-cli as the
announced fallback, and REPL transcripts for what the lowering covers already. "Objects in traits
mixed into classes" is done: the probe's NIR equals Scala Native's (§1.5).

## 1. Item 4: the gaps

Refusals on both branches, by family (the inlining stack sees more tests because 174 fewer stop at the
typer) [V]:

| family | refusal (`L/`) | 88c | 82d |
|---|---|---:|---:|
| local classes using the enclosing instance | `local_free.rs:113,118,123`, `outer_traits.rs:52` | 4 | 6 |
| creating an inner class through this prefix | `outer_args.rs:70-97` | 1 | 7 |
| subclasses of classes in classes | `outer_args.rs:37,46` | 1 | 2 |
| local traits | `local_free.rs:77` | 2 | 4 |
| lambdas and local methods in nested classes | `lambda_lift.rs:121` | 1 | 1 |
| inline methods and calls, inline calls not expanded (#82's) | `G/body.rs:51`, `G/callee.rs:29` | 13 | 5 |
| Unit ascriptions and casts, generic arrays, matches and type tests of Null, primitives and tuples, casts and boxing, local generic methods, lambdas defining classes, closure bridges | `erasure/mod.rs:160`, `type_tests/as_instance_of.rs:28-43`, `G/array.rs:81`, `patmat/plan.rs:89`, `type_tests/mod.rs:100`, `G/erased.rs:57`, `lifted.rs:60`, `beta_reduce.rs:151`, `G/closure.rs:37` | 12 | 10 |
| private `var` parameters, non-local returns, JDK-object forwarders, unrecorded lifting, super-argument locals, reflective calls | `initializers.rs:27`, `G/expr.rs:48`, `G/statics.rs:77`, `lift_names.rs:131`, `super_args.rs:31` | 0 | 7 |

### 1.1 Local classes using the enclosing instance

dotty: `ExplicitOuter.transformTemplate` gives a class an `$outer` parameter accessor and accessor
`C$$$outer()` when `needsOuterIfReferenced && referencesOuter` (`T/ExplicitOuter.scala:71-88`);
`referencesOuter` (`:263-344`) is true for a `This` of a non-static enclosing class, an `Ident` whose
prefix is one or which is hoistable with such an owner, a `New` whose class is owned by one or whose
type has such a prefix, and type tests of singletons. References become paths of outer accessors
(`:448-472`), in constructors starting from the parameter (`T/LambdaLift.scala:163-175`). Scala
Native's NIR for the probes [V]: `E$yyyy$2$::<init>(String, E)` with field `$outer`, accessor
`E$_$yyyy$$$$outer()`, `$outer.get()` in the body (p1); `C$$anon$1::<init>(C)` whose `apply`
calls `$outer.super$m()` (p3); `B$BInner` implemented by `Test$$anon$1` with a prefix path.

Ours: `local_outer.rs` already ports `needsOuterAlways` and the local-method case of
`referencesOuter`, `explicit_outer.rs:146-199` adds the parameter, field and accessor and rewrites
`This` chains (`outer_this`), `outer_traits.rs` implements the accessors of inner traits; the
refusals are reached before that [V]. Port: the `This`/`Ident`/`New` cases into `local_outer.rs`
(~60 lines); `outer_class` of a local class is its enclosing class, so `outer_this` chains cross
terms (`outer_traits.rs:16-23`, ~20); `outer_traits::implement` for a local class: the prefix
is `this.$outer` or a captured value (p8's `new b.BInner {}`), not only `This` paths (`:161-185`,
~60); `lifted_calls.rs:56-61` (a local class calling a local method that uses `this`) passes
`this.$outer` (~30); `local_free.rs` drops its three refusals. Tests: i1820b, t4300, t704, t8803;
with ProtectedAccessors also noProtectedSuper, protectedSuper. ~400 lines [I].

### 1.2 Inner classes created through a prefix, and classes nested in local classes

`outer_args::path` (`L/outer_args.rs:69-99`) accepts `this` of the current class and values on it; a
static module prefix (i2156: `a.X()` for `object a extends T`) and a `This` of an enclosing class
reached through `$outer` (t7171, nestedEq) are refused, where dotty's `outerArg` is `singleton
(fixThis(outerPrefix(tpe)))` (`T/ExplicitOuter.scala:349-373,413-437`) and `fixThis` turns a module's
`This` into a selection from outside it [V]. ~80 lines; 7 tests on the inlining stack [I].

A class nested in a local class reads the captured values of the outer local class through its
proxy fields (`T/Dependencies.scala:111-132`: `x` free in `C`'s constructor, `D` reads
`outer.x$1`), which `local_free.rs:161-168` refuses ("classes in local classes capturing values",
"calling local methods") [V]. Port: `markFree`'s intermediate class (`:134-165`) in
`dependencies.rs::mark_free`, and the reading side as an outer path to the proxy field. ~150 lines;
no refused dotty test names it today (p10 reaches it after local traits) [I].

### 1.3 Subclasses of classes in classes

`new p.C(args)` as a parent of an inner class `C` passes the outer argument to the superclass
constructor: Erasure's `typedApply` adds `outer.args(fun)` (`T/Erasure.scala:825`), the argument
for a `Super` or `New` receiver being the prefix's path, for `This` the own `$outer` parameter
(`T/ExplicitOuter.scala:413-430`) [V]. An anonymous subclass in another class is itself local and
takes `$outer` for the prefix (`referencesOuter`'s `New` case, `:314-319`); Scala Native's
`Test$$anon$1::<init>(B, Test$)` calls `B$C::<init>(b$1)` (p2) [V]. i25943 also needs the 3.9
rule that a local class's constructor inherits the free variables of the super constructor it
calls (`T/Dependencies.scala:244-252,266-270`) [V]. Ours: `outer_args.rs:35-37` refuses any parent
with an outer; `constructors.rs` builds the super call. ~250 lines; tests t8611b, i25943 (+ p2) [I].

### 1.4 ProtectedAccessors (t8611b is not one: its parent is 1.3)

`T/ProtectedAccessors.scala` (100 lines) with `T/AccessProxies.scala` (182): a reference to a
`protected` member from a class that neither is inside the member's access boundary (its package, or
the same package for Java statics, `:24-34`) nor derives from the owner gets a public forwarder
`protected$name` in the closest enclosing class deriving from the owner (`hostForAccessorOf`,
`AccessProxies.scala:173-181`), appended to that class's body (`:65-69`), its rhs spanned at the
accessed member (`:37-61`); setters as `protected$x_=` (`:111-129`); members with `@publicInBinary`
and members of traits are left alone (`:39-42`) [V]. When the host is a trait, PostTyper's
SuperAccessors makes a super accessor instead (`T/SuperAccessors.scala:179-202`); classes mixing
the trait in implement it as for `super` calls [V]. Scala Native: `q.B::protected$foo()` calling
`foo()`, `B$BIn::bar()` calling it on `$outer` (p8) [V]. Ours: nothing; `super_accessors.rs`
handles only `Super` qualifiers; the typer knows the flags (`PROTECTED`, `PROTECTEDqualified`) and
the annotation (`crates/typer/src/namer/annotations.rs:29`) [V]. New `L/protected_accessors.rs`
(~180) between FirstTransform and ExtensionMethods (`Compiler.scala:75`), plus the trait branch in
`super_accessors.rs` (~40). Tests: protectedSuper, noProtectedSuper (with 1.1); Java `protected`
members of javalib classes used from subclasses in other packages are the same mechanism [I].

### 1.5 Objects in traits mixed into classes: done

Mixin's `lazy module val x = new T$x$(this)` (`T/Mixin.scala:291-300`) is `trait_lazies.rs:39-55`;
`just nir-cmp tmp/rv/p9.scala K|T|'T?P?'` and `p6.scala C|'TR?X?'` are byte-equal, in the REPL across
inputs too [V]. Left: objects and lazy values of library traits (`trait_fields.rs:107-113`,
`library_traits.rs:20-26`), which no dotty test exercises [V].

### 1.6 Local traits, lambdas in inner classes

Local traits (t920, t920_1, t0936, llift): Dependencies counts members of local traits as local
(`T/Dependencies.scala:72-76`) and gives traits no free variables (`:151-160`); their methods take
the captured values from the caller, the trait is lifted as `Outer$Name$N` [V]. Ours refuses at
`local_free.rs:77`; `local_classes.rs` lifts classes only. ~300 lines [I].

Lambdas and local methods in a non-static inner class (dead-code-elimination): dotty's logic owner
is the inner class itself (`setLogicOwner`, `T/Dependencies.scala:194-232`), so `$anonfun$1` is an
instance method of `A$B` [V]. Ours refuses at `lambda_lift.rs:119-121` because `in_class` and
`this` assume a static class (`:123-127`, `lifted.rs:109-122`). ~120 lines [I].

### 1.7 The rest of the refusals

| refusal | tests | where | size |
|---|---|---|---:|
| ascriptions to Unit of values other than literals | i17317, i17317-b, reduce-projections, transparent-object | `erasure/mod.rs:160`; dotty types `e: Unit` as `e` against `Unit` (`T/Erasure.scala:594-597`) | 60 |
| operations on generic arrays | array-existential-bound, Course-2002-04 | `G/array.rs:67-96`: `ScalaRunTime.array_apply/update/length` for an erased `Object` array | 120 |
| matches on Null or Nothing; type tests of null, of tuples, of primitives | i5067, nullInstanceEval, i9056 | `patmat/plan.rs:89`, `type_tests/mod.rs:100-127` | 150 |
| casts: primitives, `null.asInstanceOf`, boxing | t5568, nullAsInstanceOf, i12976 | `type_tests/as_instance_of.rs:28-43`, `G/erased.rs:57` | 120 |
| local methods with type parameters | genericValueClass | `lifted.rs:60`: lift a `Poly` info | 60 |
| applications of a lambda defining classes | delambdafy-dependent-on-param-subst, -2 | `beta_reduce.rs:151`: re-own the class | 60 |
| private mutable class parameters | t2175 (and the REPL, §2.3) | `initializers.rs:27`: a `private var` parameter is a plain field with a setter | 60 |
| returns from enclosing methods, super-argument locals, unrecorded lifting, JDK-object forwarders | t8245, i2163, i24673, t8933c | `G/expr.rs:48`, `super_args.rs:31`, `lift_names.rs:131`, `G/statics.rs:77` | 250 |
| closures needing bridges | i24573 | `G/closure.rs:37`: ExpandSAMs (research 13 §4, MR 3) | 150 |

Not lowering work: "inline methods" and "a call of an inline method" (13 tests here, #82 lowers
`Inlined` trees), "reflective calls" (i18612-b, -d; `Selectable`, research 09 §5) [V].

### 1.8 Open byte-level issues (output correct, bytes differ)

- #75 `productElement`'s `IndexOutOfBoundsException(n.toString)`: `case_members.rs`, ~10 lines [V].
- #85 `Class.equals` receiver as `Object`, `as[array]` of `null` (`casts.rs` `null_conforms`),
  constant folding of `==` with constants (typer's ConstFold): ~60 [I].
- #86 the span of a bare `O.this` after Erasure (`Positioned.envelope`, `envelope.rs`), casts of a
  `null` scrutinee: ~40 [I].
- #63 patmat keeps the cast variable of a call scrutinee (`inlineVars`'s `refCount`,
  `patmat/inline.rs`): ~40 [I].
- #84 pattern-variable numbering of nested case classes and `x @ (A | B)`: one fresh counter in
  dotty's phase order (`patmat/mod.rs`): ~60 [I].
- #67 TailRec before LambdaLift: dotty's order (`Compiler.scala:115-132`); ours runs `tailrec` after
  `lambda_lift` (`L/mod.rs:134,148`). Moving it means transforming local methods too, as the
  MiniPhase does; the `$tailLocal` names then flow into the proxies. ~150, with an ADR [I].

## 2. Item 5: wiring

### 2.1 What `scalai` does today

`scalai TARGET`: a `.scala`/`.sc` target goes through `S/script.rs::main_of`: the key is a hash of
the canonical path, the text, the crate version and the Scala and Scala Native versions (`:36-42`);
`$SCALAI_CACHE/scripts/<key>/` holds `classes/` and `main` (the main classes, found by
`defines_main` reading each `.nir`'s index, `:93-117`); a miss builds with `scala-cli --power compile
--native` into `<key>.<pid>` and renames it (`:49-57,66-90`); entries unused for 30 days and
abandoned stagings older than a day are pruned (`crates/libimage/src/cache.rs:8-10`) [V]. Then
`S/main.rs::run` opens the library image (ADR 0005, `S/libs.rs::image`) and `Vm::run_main` [V]. The
REPL (`S/repl.rs`, `crates/repl`) already is the own frontend: `Universe::open_image(image)`,
`typer::Units::type_unit`, `lower::lower`, `Vm::add_nir` + `eval_module` per input (ADR 0012) [V].

### 2.2 The frontend entry and the design

`lower::compile(u, source, options) -> Result<Vec<NirFile>, String>` (`crates/lower/src/compile.rs:13`)
runs parser, desugaring, `typer::type_unit` and `lower::lower`; its errors are the strings
`scripts/dotty-lower.py:57` parses ("syntax errors", "the typer does not support …", "type errors",
"the lowering does not support … at l:c"); `bin/lower.rs:26-37` writes the files as
`<name with . as />.nir` [V]. Research 09 §5 fixed the fallback shape: one stderr line, fallback on
`Errors` too, `SCALAI_FRONTEND=strict` makes fallback an error and `=jvm` forces scala-cli [V].

Plan for `S/script.rs` [I]:
1. `compile`: on a miss, `own(&source, &staging)` first: `Universe::open_image(libs::image(jars)?)`
   (1 ms, ADR 0005), `lower::compile`, the files into `classes/`, `main` as today.
2. On `Err(what)`: `eprintln!("scalai: {file}: falling back to scala-cli ({what})")`, then `build`.
   `SCALAI_FRONTEND=strict` turns the line into the error; `=jvm` skips step 1. The key includes the
   mode, so a forced mode never serves the other's entry; `main`'s first line names the builder.
3. `.sc` files fall back: scala-cli wraps them in an object; the REPL's `wrap.rs` has the pieces for
   an own wrapper later. Files with `//> using` directives fall back too (dependencies, options).
4. `crates/scalai/tests/lowered.rs` runs `lowered.txt` through `scalai FILE.scala` with `strict`;
   one test checks the fallback line on a file the typer refuses.

Cost: `target/release/lower` (universe from the jars) takes 70 ms on `hello`, 160 ms on `moderate`
end to end [V]; the image saves ~16 ms (ADR 0005), the rest is the typer and the lowering, measured
in the MR against the ≤ 50 ms criterion; a hit costs VM time only, as today [I]. ~250 lines.

### 2.3 The REPL

Piped into `scalai` (`tmp/rv/repl-probe.txt`), these work now [V]: `lazy val` at top level
(`lazy val lz: Int`, initialized on use), in classes, objects and methods; `enum Color { case Red,
Green }` and `enum Shape { case Circle(r: Int) … }` with `Shape.Circle(2)`; `case class Box[A](a: A)`;
`class M(var p: Int)` with `m.p = 2`; `trait T { object O … }; class K extends T; new K().O.f`.
`Shape.values` is "member not found" as in dotty (the scaffolding exists only for enums whose
cases are all singletons, `ref/scala3/.../ast/DesugarEnums.scala:147`, `desugar/enums.rs:33-35`). Still refused: `class A(private var p: Int)`
(`initializers.rs:27`, `tests/repl/unsupported.check`), abstract `var`s (`getters.rs:53`), `enum E
extends java.lang.Enum[E]` (typer). The ROADMAP line "lazy vals, enums, generic case classes and
mutable class parameters wait for the lowering" is stale [V]. Work: transcripts `tests/repl/
lazyvals.check` and `enums.check`, generic case classes in `classes.check`, recorded with `just
repl-oracle`; the ROADMAP; `private var` parameters with 1.7. ~150 lines.

## 3. MRs

Each gated by `just gates`, `just dotty-lower-report` and `just snapshot-diff programs lower` without
worsened tests, and a `tests/programs` entry in `lowered.txt` compared with `just nir-cmp` [I].

| MR | scope | unblocks (88c / 82d) | risks |
|---|---|---|---|
| 1 wiring (~400) | §2.2 steps 1-4, `docs/conformance.md`, ROADMAP | the ≤ 50 ms exit criterion becomes measurable | typer or lowering bugs now reach scripts: `strict` in CI, the fallback line in every other run |
| 2 REPL transcripts (~150) | §2.3 | — | none |
| 3 outer instances of local classes (~600) | §1.1, §1.2 prefixes; program `localouter` | 4 / 11 | `referencesOuter` over-approximates: an `$outer` dotty adds but never reads still changes the constructor signature; `$outer` of anonymous classes dropped by `DropOuterAccessors` |
| 4 subclasses of inner classes (~400) | §1.3, i2163's super-argument locals; program `innersubclasses` | 1 / 4 | the outer argument's prefix path inside constructors; Constructors' rewiring of `$outer` |
| 5 ProtectedAccessors (~350) | §1.4; program `protectedaccess` | 0 / 2 (with 3) | access boundaries of qualified `protected[p]`; Java statics |
| 6 local traits, lambdas in inner classes, classes in local classes (~700) | §1.6, §1.2 second part | 3 / 5 | free variables through trait methods; `logicOwner` narrowing vs our static-first lifting |
| 7 small refusals (~600) | §1.7 but closure bridges | 10 / 14 | `ScalaRunTime` array helpers on primitive arrays; `Erased::Unit` ascriptions in `adapt` |
| 8 byte issues (~400) | §1.8 but #67; #67 after its ADR | — | `nir-cmp` on the whole `tests/programs` |

Order: 1 and 2 first (they change no lowering and make scripts measurable), then 3 → 4 → 5 (each
builds on the outer paths of 3), 6, 7, 8. About 3,600 lines in all, every MR under ~700 [I].
