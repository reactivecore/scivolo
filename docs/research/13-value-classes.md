# 13 — Lowering of value classes (#88)

Date: 2026-10-12. Claims are [V] verified by reading the code or running it, [I] inferred. Paths:
`T/` = `ref/scala3/compiler/src/dotty/tools/dotc/transform/` (3.9.0), `C/` = `.../dotc/core/`, `N/` =
`ref/scala-native/nscplugin/src/main/scala-3/scala/scalanative/nscplugin/` (0.5.12), `L/` =
`crates/lower/src/phases/`. ROADMAP "Where we are and what's next" item 2; the typer half is #83
(merged). Reference NIR: `tmp/vc/probe.scala` (a value class `Meter(value: Double)` with methods,
a generic `Wrap[A]`, a universal trait, `==`, `hashCode`, `Array[Meter]`, a lambda, a type pattern,
casts, `3.compare(4)`) compiled by Scala Native into `tmp/sn/3.9.0/probe/`; both uncommitted.

**Recommendation.** Keep the lowering's design (trees carry unerased `tpe`, `Erased` is computed on
demand) and add dotty's semi-erasure as `Erased::Value(class, underlying)` with `erase_value`;
port ExtensionMethods as one pass before Getters, extend `adapt` with box and unbox, run
VCElideAllocations as a peephole inside erasure, synthesize `hashCode`/`equals`. Four stacked MRs;
the first already matches Scala Native's NIR for a plain value class and lifts the catch-all
refusal "members value classes inherit" (`L/value_classes.rs:55`), which today fires on every
`new V(x)` and every inherited member selected on a value-class receiver [V].

## 1. What dotty and nscplugin do

| phase | lines | does | NIR of the probe [V] |
|---|---:|---|---|
| Desugar `ast/Desugar.scala:1051` | — | an empty companion for a value class without one | `Meter$` exists; `crates/syntax/src/desugar/class_def.rs:18,42-46` does it already [V] |
| SyntheticMembers `T/SyntheticMembers.scala:97-118,162,293-342` | ~40 | `valueSymbols` = `hashCode`, `equals` unless defined; `hashCode` = `x.hashCode()` for a primitive underlying else `java.util.Objects.hashCode(x)` (:334); `equals` = `that match { case x$0: C => this.x == x$0.x; case _ => false }` without the `eq` test (:312) and without `canEqual` (:304, the class is final) | `Meter::hashCode()`, `Meter::equals(Object)` forwarders; `Meter$::hashCode$extension(double)` = `box[Double]; hashCode()`; `equals$extension(double, Object)` = `is[Meter]`, null test, `as[Meter].value()`, `feq` |
| ExtensionMethods `T/ExtensionMethods.scala` + `T/FullParameterization.scala` | 219 + 269 | for every real method but constructors, super accessors, inline and macro (`T/ValueClasses.scala:15`): a `final` `m$extension` in the companion's module class, params `($this: V)` then the method's clauses, type params the method's then the class's (:90); its body is the original's with `this` → `$this`, params substituted, owners changed (:147-215); the original becomes `V.m$extension[Ts](this)(args)` (:222-242); defs appended to the companion body (:150-155); `@tailrec` moved, constructor access flags and `Local` reset (:97-112); `u2evt$`/`evt2u$` cast methods (:82-91) only make Erasure's trees typecheck and vanish in ElimErasedValueType — not needed without retyping [I]. Runs after FirstTransform, before ElimByName and PatternMatcher (`Compiler.scala:75`) | `Meter::$plus(double): double` calls `Meter$::$plus$extension(double, double)` on `this.value()`; `$this` keeps its name as a local; static forwarders `Meter::static $plus$extension` with `inlinehint` (`N/NirGenStat.scala:216`) |
| VCInlineMethods `T/VCInlineMethods.scala` | 120 | `e.m[Ts](args)` → `V.m$extension[Ts, Cs](e)(args)`, `val ev$N = e` first if V has type params and `e` is not stable | ported: `L/value_classes.rs` (179), for library and script classes; `extension()` there defines the symbol lazily because library TASTy predates the phase [V] |
| TypeErasure `C/TypeErasure.scala:224,972-1007,1015-1020` | ~60 | `valueErasure`: `V[As]` → `ErasedValueType(V, erasure of the underlying as seen from V[As])`; a generic underlying that erases to a primitive gives its box, an `Array` underlying erases as at the definition (:995-1000; `crates/tasty/src/erasure/applied.rs:31-49` has both for full erasure [V]); a constructor's result is not semi-erased, so `new V(x)` has type `V` | `Test$::len(String)` for `n: Name`; `val w = new Wrap(7)` is `box[Integer] 7` |
| Erasure `T/Erasure.scala:272-407,733` | ~130 | `adaptToType`: EVT → other: `box` = `new V(cast(e, U))` (:275); other → EVT: `unbox` = `cast(e, V).value()` under `if e eq null then null.asInstanceOf[U] else …` unless `e.tpe <:< V` (:292-317); EVT → EVT: cast. `typedSelect` boxes any qualifier of EVT type (:733): inherited members (`toString`, `getClass`, `RichInt.compare`, universal trait methods) and `value` itself | `3.compare(4)` = `classalloc RichInt; <init>(intWrapper(3)); compare(box 4)`; `case mm: Meter` binder = null test, `as[Meter].value()`; `n.shown` on `Name with Shows` = `classalloc Name` then `shown()` |
| Erasure `adaptClosure` `T/Erasure.scala:439-505` | ~70 | `$anonfun$adapted` when a parameter or the result erases differently from the SAM's, body `adapt`ed both ways | `Test$::$anonfun$adapted$1(Object): Object`, `alwaysinline`: null test, `as[Meter].value()`, call, `classalloc Meter` |
| Bridges `T/Bridges.scala:23,64` | — | pairs by signature after ElimErasedValueType, bodies adapted on semi-erased types | mixin-primitive-on-generic-4: `Foo.value1(): boolean` bridge of `Bar.value1(): Object` unboxes [I] |
| ElimErasedValueType `T/ElimErasedValueType.scala` | 143 | EVT → underlying in every info and tree type; drops the cast methods; double-definition check | nothing to port: `erased_method` (`crates/tasty/src/erasure/mangle.rs:71`) is the full erasure; codegen maps `Value` to the underlying's NIR type [I] |
| VCElideAllocations `T/VCElideAllocations.scala` | 55 | `new V(u1) == new V(u2)` → `u1 == u2` unless V has a non-synthetic `equals` (matching member at erasure); `new V(u).value()` → `u` | `m == new Meter(1.5)` is `feq[double]`; `(new Meter(x)).value` leaves `x` |
| TypeTestsCasts `T/TypeTestsCasts.scala:275,295` | — | `isInstanceOf[V]` tests the class; `asInstanceOf[V]` returns `expr`, adaptation unboxes | `any(m)` = `is[Meter]`; `null.asInstanceOf[Foo]` (t5866) = `unbox[Double] null` |
| Getters `T/Getters.scala:68`, ExpandPrivate `T/ExpandPrivate.scala:61-71` | — | a `private[this]` parameter of a value class keeps its getter; the accessor becomes public without a name change | `Name::s()`, `alwaysinline` (accessor, `N/NirGenStat.scala:334`) |
| GenNIR `N/NirGenExpr.scala:386,420,2195-2270`, `N/NirGenType.scala:100` | ~90 | closure classes `ensureUnboxed` (cast then `value()`, no null test) and `ensureBoxed` (`classalloc` + ctor) on the type entering ElimErasedValueType; EVT is not primitive | lambda classes with a value-class SAM (`MySam[Meter]`, i24573's `ConVCVC`) |

Not needed: `u2evt$`/`evt2u$`, ElimErasedValueType's checks, InterceptedMethods (nothing for value
classes [V]). About 600 Scala lines of new logic beyond what `L/value_classes.rs` has.

## 2. Representation and the code it touches

- **`tasty`**: `Erased::Value(String, Box<Erased>)` and `Universe::erase_value(t)`: `erase` but a class
  with `underlying` becomes `Value(binary_name, erase(underlying as seen from t))`; `This(c)` of a value
  class stays `Class(c)` (dotty keeps `ThisType`, `C/TypeErasure.scala:251`, hence `this.value()` in
  the forwarders without boxing) [V]. `erase`, `erase_boxed`, `erased_method` and the mangling stay.
  The issue's `Type::Boxed(TypeId)` types trees whose value is the object: `new V(x)`, `box` results,
  casts to `V`; `erase_value(Boxed(V))` = `Class(V)`. Alternative: `L/adapt/mod.rs::erased_type`
  maps constructor applications and `explicit_casts` to `Class(V)`; `Boxed` survives trees moving
  into `Block`s and `If`s and costs about 25 `match`es on `Type` [I].
- **`L/adapt/mod.rs::adapt`** (dotty :385): arms before the primitive ones: `(Value, other)` → `new
  V(cast(t, U))`; `(other, Value(V, U))` → `cast(t, Class(V)).value()` in the null test unless
  `erased_type(t)` conforms to `Class(V)` or `t` is `This`; `(Value, Value)` differing → `cast`.
  `erased_type` uses `erase_value` on infos and method results; `erased_method` stays for names [I].
- **`L/erasure/mod.rs::typed_qualifier`**, `L/casts.rs::receiver_type`: a `Value` qualifier adapted
  to the owner's class boxes it (:733). New `L/vc_elide.rs` after `calls::typed_apply`: `new
  V(u).value()` → `u`; `new V(a) == new V(b)` → `a == b` (`Lower::equal`) unless V declares `equals`.
- **`L/extension_methods.rs`** (new, ~150): in `run` after the `first_transform` loop (`L/mod.rs:98`),
  before `initializers`/`getters` (:101): for each value class, define the `m$extension` symbols in
  declaration order into `Lower.extensions` (reuse `value_classes::extension`, :154); move the rhs
  node into a def owned by the extension with `$this` and fresh params (symbol map as
  `L/lifted.rs:155 substitute`; nested defs re-owned; `Return` labels retargeted); the original
  gets a fresh forwarder body (`L/methods.rs:33 forwarding_def` pattern) with the span of the old
  rhs (dotty :241); append the defs to the companion template (its `TypeDef` is in `classes`). No
  tree copy is needed: the body moves, the forwarder is new [I].
- **`L/synthetic.rs`** + new `L/value_members.rs` (~90): `hashCode`/`equals` for classes with
  `underlying`; `equals_body` (`L/case_equality.rs:16`) with a flag dropping `eq` and `canEqual`;
  `Objects.hashCode` in `definitions/`. Runs before ExtensionMethods, so both get extensions.
- **`L/getters.rs`**, `L/expand_private.rs`: the two value-class rules above.
- **`L/type_tests/mod.rs:72-77`**, `as_instance_of.rs:20`: lift the three refusals; a cast to a
  value class returns `expr` and lets `adapt` unbox; `sensical.rs:75` already knows value classes.
- **`L/closures.rs:54`**, `L/adapted.rs`: `$adapted` also for `Value` parameters or results against a
  reference SAM; `adapted_def` adapts each argument and the result with `adapt`;
  `codegen/closure_methods.rs:83,99` `ensure_unboxed`/`ensure_boxed` for `Value`.
- **`L/bridges.rs:29-30,146-157`**: pair by `erased_method`, adapt with `erase_value` of both infos.
- **Codegen**: `codegen/erased.rs::gen_type` and the 16 `Erased::` matches map `Value(_, u)` to `u`.

## 3. Tests

74 files in 71 of dotty's `tests/run` say `extends AnyVal` (61 single-file; directories colltest5,
colltest6, gestalt-optional-inline, i8001, java-vc-overload, lst, t7859, t8017, t8133,
unroll-value-class) [V]. `target/release/lower` on the 61 today [V]: 4 lower (classof, classTags,
i13216, value-class-extractor-seq); 43 stop at the typer (11 of them at transparent inline calls:
Meter, MeterCaseClass, vc-equals, t6534, t13033, … need this work next); 14 at the lowering: 12
"members value classes inherit" — getClassTest-valueClass, i24573, i4563, lambda-sam-bridge,
mixin-primitive-on-generic-4, -5, phantomValueClass, t5608, t6260, t6500, t6574b,
value-class-extractor —, t5866 ("casts between primitive values or to references": `null
.asInstanceOf[Foo]`), genericValueClass (local methods with type parameters, not this issue).
The conformance count of 14 (`docs/conformance.md:158`) is the same set at an older baseline.

## 4. MRs

| MR | scope | checks | unblocks | risks |
|---|---|---|---|---|
| 1 `#88 value classes lowered` (~1200) | `Erased::Value`, `erase_value`, `Type::Boxed`; `extension_methods.rs`; value `hashCode`/`equals`; `adapt` box and unbox; receiver boxing; `vc_elide.rs`; Getters and ExpandPrivate rules; refusal lifted; `tests/programs/valueclasses` (the probe without lambda, patterns, casts) in `lowered.txt` | `just nir-cmp tests/programs/valueclasses/valueclasses.scala Meter`, `Meter?`, `Name`, `Wrap?`, `Test?`; `just gates`; `just dotty-lower-report` and `just snapshot-diff programs lower` without worsened tests | t5608, phantomValueClass, t6574b, t6500, getClassTest-valueClass, value-class-extractor | forwarder and moved-body spans; `ev$N` numbering; `This` vs `Boxed` in `erased_type`; Mixin forwarders of universal traits over a value-class receiver; private `$extension` of t6574b called from the class (dotty :72 expands it) |
| 2 `#88 type tests and patterns` (~500) | `type_tests` refusals lifted, binders unboxing (`case mm: Meter`), `null.asInstanceOf[V]`, `Array[V]`, value-class extractors; program extended | the same, plus a second `Test?` | t5866, i4563 | `sensical` with value classes; `is[]` and `as[]` dropped by codegen when the value's NIR type is known already (the probe's `o.asInstanceOf[Meter]` has no `as`) |
| 3 `#88 closures and bridges` (~600) | `$adapted` with `Value`, `ensure_(un)boxed` in codegen, bridges adapting semi-erased types, SAM traits over value classes | program with `(a: Meter) => a.twice` and `MySam[Meter]`; `Test??Lambda?1` | lambda-sam-bridge, mixin-primitive-on-generic-4, -5, i24573, t6260 | `additionalAdaptationNeeded` against the specialized `JFunction` choice (`L/adapted.rs:14`); `alwaysinline` on `$adapted`; i24573 also needs its "expanded" SAM closures |
| 4 `#88 hardening` (~400) | the rest of the corpus as the typer catches up (vc-equals, Meter, t6534: user `equals`, universal traits with `equals`); `@targetName` on value-class methods (i13216 lowers already — compare its NIR); generic underlying arrays (i8001); `docs/conformance.md`, ROADMAP | `just dotty-lower-report` | typer-blocked tests | none specific |

Each MR under ~1500 diff lines, files under 200 lines (`extension_methods.rs`, `vc_elide.rs`,
`value_members.rs`, `tasty/src/erasure/value.rs` are new); every MR gated by the oracle program.
