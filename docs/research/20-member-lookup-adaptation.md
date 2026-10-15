# 20 — Typer refusals "member not found", "adaptation", "RefinedTypeTree"

Date: 2026-10-15. Claims are [V] verified by reading the code or running it, [I] inferred. Paths:
`T/` = `ref/scala3/compiler/src/dotty/tools/dotc/typer/` (3.9.0), `C/` = `.../dotc/core/`, `D/` =
`.../dotc/ast/`, `Y/` = `crates/typer/src/typer/`, `Yn/` = `crates/typer/src/namer/`, `S/` =
`crates/tasty/src/`, `L/` = `crates/lower/src/`. Measured on main aed90cdc with a fresh
`just dotty-lower-report` (855 pass, 548 typer refusals, 29 lowering) [V]. `m01`–`m12`, `a01`–`a09`
name reductions of the tests, run through `just typer-dump` [V]; none was compared with
`just typer-compare`, the oracle shapes below are read from dotty's code [I].
Follows [research 19](19-typer-groups-deg.md).

**Recommendation.** The three reasons are the first refusal of 92 run tests (43 + 30 + 19) [V]
but name no single construct: the 43 "member not found" split into packages declared later in
the same file (7, §2.1, a 20-line namer fix), members of refined types (3 + the 7 type-member
refinements of §2.8, one `members_within` arm), generic tuples (7, §2.3), implicit views the search
misses (6, §2.4), inline `unapplySeq` extensions (9, #82), JDK-only API and `scala.xml` (7, out).
The 30 "adaptation" refusals are four small rules of dotty's `typedApply`/`adapt` (§2.5–2.7: infix
tuple arguments 4, inherited result types as seen from the class 4, a view on the qualifier of a
failed application 3, Java `Object` parameters 2–3) plus match types over tuples (6, with §2.3) and
singles. "RefinedTypeTree" is one typer feature with two halves: refinements of type members
(7 tests, typer only) and structural `def`/`val` refinements (8, `Selectable`, lowering's
reflective calls, out for now) plus `Mirror` refinements (4, #43). Six MRs under ~400 lines in §3,
about 45 of the 92 tests; the best ratios are §2.1 and §2.5.

## 1. The refusals

First refusals of `just dotty-lower-report` [V], causes by reading every test at its offset [V]:

| run | reason | `Y/` site | § |
|---:|---|---|---|
| 7 | member not found: package declared later in the unit | `Yn/mod.rs:32-35` | 2.1 |
| 3 | member not found: `m.MirroredElemTypes` on a refined `using` type | `members.rs:92-95` | 2.2 |
| 7 | member not found: `*:` on `Tuple()`, `_1` on `Int *: EmptyTuple` | `members.rs:54-76`, `select.rs:58-66` | 2.3 |
| 6 | member not found: a view the search misses | `implicits/conversions.rs:44-92` | 2.4 |
| 9 | member not found: `mac"…"` with an inline `unapplySeq` extension (i8577a–i) | `unapply.rs:92-96` | out, #82 (research 19 §2.6) |
| 7 | member not found: JDK API javalib lacks, `scala.runtime` Java classes, `scala.xml` | — | out |
| 4 | member not found: `into[T]` (2), case-class companion shadowed by a `def` (1), `.type` of a val of a local object (1) | — | out |
| 4 | adaptation: infix `a op (b, c)` | `apply.rs:10-30` | 2.5 |
| 4 | adaptation: inferred rhs against an inherited result type | `Yn/inherited.rs:64` | 2.6 |
| 3 | adaptation: `"a b".split(' ')` (view on a failed application) | `apply.rs:26-31`, `overloads.rs:100` | 2.7 |
| 3 | adaptation: Java `Object` and `T*` parameters (i8977, i9439, t3199b) | `applicability.rs` | 2.7 |
| 6 | adaptation: `Tuple.Head`, `Tail`, `Union`, `Zip` match types (tuples1b, toList, tuple-ops, i23897, i4600b, i9530) | — | 2.3, out |
| 10 | adaptation: singles (§2.9) | — | mostly out |
| 7 | RefinedTypeTree: `T { type X = Y }`, `{ type X <: p.type }` | `type_trees.rs:104-107` | 2.8 |
| 8 | RefinedTypeTree: structural `def`/`val` members (`reflectiveSelectable`, user `Selectable`) | `type_trees.rs:104-107` | 2.8, out |
| 4 | RefinedTypeTree: `Mirror.Product { type MirroredType = … }` written by hand | `type_trees.rs:104-107` | #43 |

## 2. Causes

### 2.1 Packages declared later in the unit (7 tests)

`object Test { … new foo.Bar … }; package foo { class Bar }` (`m02`) refuses `foo.Bar` [V]. Cause
[V]: `index` (`Yn/mod.rs:25-46`) only names the package of a nested `PackageDef` (`typed_pid`,
`:33`) and leaves its statements to `typed_package_def` (`Y/stats.rs:10-29`, `index` at `:19`),
which runs when the clause is typed, after the clauses before it. dotty's `indexExpanded`
(`T/Namer.scala:439-446`) indexes the statements of a package clause while indexing the enclosing
one (`index(pcl.stats)(using ctx.packageContext(pcl, pkg))`, `:443`), so every top-level symbol
of the unit exists before any statement is typed. Port (~20 lines): `index` enters the clause's
statements with `Ctx { owner: pkg, level: Members(pkg) }` and `typed_package_def` reuses that
level instead of indexing again (keep the level in a map by tree, like `import_levels`); the
`check_definition`/double-definition checks already key by symbol. Tests [V by offset]: Meter,
MeterCaseClass (`_root_.b.Printable`, `b` declared at the end), i2964e, i3000b, t5105, t1987
(`bip.bar.duh`, `bar` a package object extending traits), i13332shapeless (`import shapeless.impl.*`
before `package shapeless`, then #43 mirrors). Meter/MeterCaseClass continue into value classes
(#88), t1987 into package-object inheritance [I]; about 4 pass.

### 2.2 Members of refined types (3 tests, plus §2.8)

`def f[T](using m: Mirror.ProductOf[T]) = … m.MirroredElemTypes` (`m08`) refuses [V].
`Mirror.ProductOf[T]` is `Mirror.Product { type MirroredType = T; …; type MirroredElemTypes <: Tuple }`
in the library's TASTy (`Type::Refined`, `S/types/decode/mod.rs:163`); `members_within`
(`Y/members.rs:24-97`) has no `Type::Refined` arm, the default (`:92-95`) takes the parent's class
members, and `Refined`'s `info` for the name is never consulted, so the type member is found with
the parent's bounds or, for a name the parent lacks, not at all [V, by reading; the exact failure
of `m08` is in the `is_type_symbol` filter of `select.rs:34-38` or the empty result, [I]]. dotty's
`findMember.goRefined` (`C/Types.scala:900-939`): the parent's denotation with its info refined
(`rinfo` alone for a type alias, `:904-911`; `pinfo & rinfo` otherwise). Port (~40 lines): an arm
`Type::Refined(parent, n, info)` that, for `name == n`, returns the parent's member with the refined
info (a term member of a structural refinement has no symbol: out of the subset, "structural
member"), else recurses into `parent`; `ref_type` (`select.rs:114`) must then take the member's
type from the refinement (`S/types/widen.rs` `member_info` sees `Refined` already? [I, check
`seen_from.rs`]). Tests: deriving-constructor-order, i7049, poly-kinded-derives (all continue into
`Tuple.Elem`/`constValue` of #43 [I]); §2.8's seven.

### 2.3 Generic tuples (7 member + 6 adaptation tests)

Three distinct holes [V]:

- `1 *: Tuple()` (`m01`, `m05`): `Tuple.apply()` has type `scala.Tuple$package.EmptyTuple` (the
  alias of `EmptyTuple.type`); `x0.size` on it types (`m05b`) but `x0.*:(1)` is "member not found",
  while `1 *: EmptyTuple` with the object's singleton type types (`m07`, `m04`) [V]. The `*:` of
  `scala.Tuple` is `inline def *: [H, This >: this.type <: Tuple](x: H): H *: This`; the alias path
  goes through `members.rs:54-76` (`Kind::Type` → `info` → `Alias` → `TermRef` → widen). Which step
  drops `*:` is unknown [I]: compare `type_members_in` for `size` and `*:` on `m05`. 5 tests:
  i13968, tuple-product, tuples-empty, tuples1, tuples1a.
- `(1 *: 2 *: EmptyTuple)._1` (`m07`): dotty's `trySmallGenericTuple` (`T/Typer.scala:880-888`,
  `normalizedTupleType` `C/TypeUtils.scala:151-157`, `tupleElementTypesUpTo` `:66-84`) casts a
  `*:` chain of ≤ 22 elements to `TupleN` and selects there; ours has no such step (`select.rs`
  tries conversions and extensions only). Port ~50 lines in `select.rs:45-57` before `viewed`:
  `tuple_elements(t)` (the subtyper has the inverse, `S/subtype/hk.rs:176-195`) and a `Typed`
  cast to `TupleN[…]` (`qual.cast`, lowering sees an `asInstanceOf`). 2 tests: genericTupleMembers,
  i14215.
- `xs.head` on `(T, T)`, `(1, 2, 3).toList`, `a.zip(b)`, `x.tail` (`a03`, tuples1b, toList,
  tuple-ops, i23897): the results are match types `Tuple.Head[(T, T)]`, `List[Tuple.Union[…]]`,
  `Tuple.Zip[…]`, which dotty reduces (`C/MatchTypeTrace`, `TypeComparer.matchCases`
  `C/TypeComparer.scala` `reduceMatchType`), and the typer refuses as "adaptation" when the
  unreduced type is checked against the declared one. `Type::Match(bound)` keeps only the bound
  (`S/types/decode/mod.rs:164-169`); reduction is a new subsystem (ROADMAP "Tuple", 15 tests,
  plus `scala.compiletime`, 12). Out of this note; the "Tuple" reason deserves its own research.

### 2.4 Views the search misses (6 tests)

- `x.mkString(", ")` for `x: Array[_]` (`m09`, array-addition; `x.headOption` on
  `Array[? >: AnyRef]`, i23179) [V]: `genericArrayOps[T](xs: Array[T])` against `Array[?]` needs
  wildcard capture; dotty `inferView` → `captureWildcards` (`T/Inferencing.scala:612`, called from
  `adaptToSubType`'s `captureWildcardsCompat` `T/Typer.scala:5006-5008` and from `tryExtensionOrConversion`
  [I]). Ours: `try_conversion` (`implicits/conversions.rs:142-199`) checks `conforms(arg_tpe, param)`
  with the wildcard argument unchanged (`:174`). ~60 lines: capture `Bounds` arguments of the
  argument type into fresh skolems/type variables before the view is tried [I]. 2 tests.
- `1000.seconds` (`m10`, `m10b`; deadlock, duration-coarsest) [V]: `DurationInt` is an `implicit
  final class … extends AnyVal with DurationConversions` in `package object duration`; with the
  explicit import the view is still not found, and `DurationInt(4).seconds` stops at "overloaded
  reference with a polymorphic alternative" (`m10c`), so the problem is the implicit class's
  companion `apply` overloads or the value-class conversion, not the import [I]. Needs a
  `typer-compare` of `m10c`. 2 tests (deadlock then needs threads, out).
- `x.tap(…)` with `import scala.util.chaining.given` (`m11`; i10889) [V]: the `given` selector must
  import the implicit `scalaUtilChainingOps` — dotty's `importedImplicits` takes implicits and
  givens under a `given` wildcard (`T/ImportInfo.scala:131-144`, `:141`), ours only symbols flagged
  `GIVEN` (`Y/imports.rs:172-179`) [V]; the named import types (`m11b`). ~10 lines. 1 test (then
  context functions in `f(10)(Ctx ?=> …)`, [I]).
- `"asdf".copy(b = 3)` through `implicit def toA[T](t: T)(implicit f: T => Int): A` (t3346g): a
  view with an implicit parameter list whose argument is a function `T => Int` from an implicit
  value; `with_implicit_args` inside `try_conversion` (`conversions.rs:183`) [I]. 1 test, group A.

### 2.5 Infix operators with tuple arguments (4 tests)

`b put (4, 4)`, `xs slice (1, 2)` (`a09`; collections, colltest1, t4813, t5045) refuse at the
tuple [V]. The desugaring marks the application `Attrs::INFIX_TUPLE_APPLY`
(`crates/syntax/src/desugar/exprs.rs:137`, dotty `D/Desugar.scala:1786` `ApplyKind.InfixTuple`),
and dotty's `typedApply` retries with the `tupledDual` prototype — the tuple's elements as
separate arguments — when the function is not unary (`T/Applications.scala:1321-1322`,
`needsTupledDual` `:1672-1681`, `ProtoTypes.scala:596-608`). Ours: `typed_apply`
(`Y/apply.rs:10-30`) builds one `FunProto` from the argument list as written. Port (~40 lines):
when the attribute is set, the single `Tuple` argument has > 1 elements and the resolved method
(or every alternative) is not unary, type with the elements as `args`; dotty decides per
alternative in `resolveOverloaded` (`:1676`, `isUnary`), so the arity check belongs in
`overloads.rs` as a second prototype [I]. Tests then continue: collections (large, [I]),
colltest1, t4813, t5045 (`"…".r("year", "month")`, `StringOps.r(String*)`).

### 2.6 Inherited result types as seen from the class (4 tests)

`trait It { type T; def next: T }; class SI extends It { type T = Char; def next = { …; 'c' } }`
(`a07`; absoverride, t1939, t6070, typeclass-derivation2d's `_MonoType = this.type`) refuses the
rhs [V]. `inherited_result` (`Yn/inherited.rs:14-44`) takes `member_info(This(SI), m)` (`:64`) and
the rhs is typed against it (`Yn/complete.rs:84-85`); the expected type keeps `It`'s `T` in a form
the subtyper does not resolve through `SI`'s alias [I: either the prefix stays `It.this` or
`Sym(_, It.T)` without prefix; `a07` through `typer-compare` tells]. dotty's `inferredResultType`
(`T/Namer.scala:2168-2205`) takes `iDenot.info … .asSeenFrom(site, cls)` with
`site = sym.owner.thisType` (`:2173`, `:2200`), so the type is `SI.this.T`, which dealiases to
`Char`. Ours already calls `member_info(This(SI), m)` = `as_seen_from(info, This(SI), It)`
(`S/symbols/seen.rs:128-151`, `:192-198`), which rewrites `It.this` to `SI.this` (`:135-138`)
[V]; so either `It.T`'s reference in the TASTy-free script info carries no prefix
(`Type::Sym(None, T)`, which `as_seen_from` leaves alone) or `conforms(Char, SI.this.T)` fails to
dealias through the subclass's `type T = Char` [I]. Port (~20–40 lines) at `Yn/inherited.rs:64`
and `S/subtype/refs.rs` `meaning` once `a07` is traced. 4 tests; absoverride then
needs `abstract override` (research 19 §2.5), t6070 and t1939 likely pass.

### 2.7 Adaptation after a failed application (3 + 3 tests)

- `"a b".split(' ')` (`a08`; OrderingTest, comparable-comparator, richs) [V]: `String.split(String)`
  is a member but not applicable to a `Char`; dotty's `typedApply` then inserts a view on the
  qualifier (`tryWithImplicitOnQualifier` `T/Applications.scala:1374-1386`, `:1495-1497`;
  `tryInsertImplicitOnQualifier` `T/Typer.scala:4278-4301`) and finds `StringOps.split(Char)`. Ours
  reports "no applicable alternative" from `overloads.rs:100` through `with_apply`/`typed`
  (`apply.rs:26-31`) and never retries. Port (~60 lines): on `Err` of the function part, if `fun`
  is a `Select` whose qualifier is a value, `viewed(qual, Goal::Member(name))` and retype the
  application with the converted qualifier, once (dotty's `InsertedImplicitOnQualifier`
  attachment). 3 tests, all three then continue into other library calls [I].
- `System.out.printf("…", pi)` (`a01`; i8977), `Collections.addAll(coll, 5, 6)` (i9439),
  `Arrays.asList(Array(1, 2, 3): _*)` (t3199b) [V]: dotty types a Java signature's `Object` as
  `FromJavaObject`, an alias of `Object` that `<:<` treats as `Any` (`C/Definitions.scala:333-428`,
  `C/TypeComparer.scala:350-351`, `:3080`), so `Double <: Object...` holds and erasure boxes. Our
  javalib is Scala-defined TASTy (Scala Native's), so the typer sees `AnyRef*`; `is_java_class_name`
  (`Y/eta.rs:177-183`) already names `java.*`, `javax.*` and the library's Java classes, and
  `Yn/inherited.rs:84-85` already equates `Object` and `Any` for them. Port (~40 lines): in
  applicability and `adapt`, a formal `java.lang.Object` (or its repeated form) of a member whose
  owner `is_java_class_name` accepts any type (`applicability.rs`, `adapt.rs:83`); the lowering
  boxes primitives at `Object` parameters already (NIR call signature) [I, check with
  `just lower-run a01`]. 2 tests; t3199b's `Array[Int]: _*` to a Java `T*` is `ElimRepeated`'s
  copy, out.

### 2.8 Refined type trees (19 tests)

Every `T { … }` is refused at `type_trees.rs:104-107` [V]. By refinement kind [V, all 19 read]:

- **Type members** (7): `type Aux[T, R] = Generic[T] { type Repr = R }` (byname-implicits-17,
  byname-implicits-6), `Sam1 { type T = String }` as the type of a lambda (i18315),
  `Ops[F, A] { type T = Func[F] }` as a conversion result (i23409), `SuccOpt { type Refined <: nat.type }`
  as an extractor result (fully-abstract-nat-5/6/7). dotty: `typedRefinedTypeTree`
  (`T/Typer.scala:2704-2726`) desugars to a class with the refinements as members
  (`D/Desugar.scala:2507` `refinedTypeToClass`), types it, and `assignType` folds the members into
  nested `RefinedType(parent, name, info)` (`C/Types.scala:3270`); a refinement must refine an
  existing member (`T/Checking.scala` `checkRefinementNonCyclic`, "refines no member" is an error
  via `TypeAssigner`), and `isPrecise`/alias members are looked up by `goRefined` (§2.2). Port
  (~150 lines typer): `typed_refined_type` in a new `Y/refined_types.rs`: type the parent, for each
  `TypeDef` refinement type the bounds or alias in a context whose owner is a fresh refinement class
  symbol (so `nat.type` and `this`-less references resolve), build `Type::Refined` from the outside
  in, check each name is a type member of the parent; `members_within` §2.2 supplies lookup;
  subtyping of `Refined` exists (`S/subtype/third.rs:17`, `fourth.rs:25`) [V]; `and_members`,
  `widen_union.rs:25,118` already map `Refined` [V]. Lowering: erasure takes the parent
  (`S/erasure/mod.rs:83`) [V]; nothing else. Tests: i18315 (then SAM against a refined type: `to_sam`
  must see through `Refined`, `closures.rs`), i23409 (then G1 HK), byname-implicits-6/17 (then by-name
  implicit search, A, [I]), fully-abstract-nat-5/6/7 (then `x0.Refined` type selections on a val,
  `asInstanceOf`; 2–3 may pass).
- **Structural members** (8): `{ def test(a: String, b: Int): Unit }`, `{ val a: Int }`,
  `{ def a_=(x: Int): Unit }` through `reflectiveSelectable` (i4496a, i4496b, i4528, i9404,
  structural, structural-compat) or a user `Selectable` with `applyDynamic`/`selectDynamic`
  (i17761, structural-contextual). The typer side is `trySelectable`/`typedDynamicSelect`
  (`T/Typer.scala:939-957`, `T/Dynamic.scala`); the lowering needs reflective calls, refused today
  (`just dotty-lower-report`: "reflective calls", 2). Out until the lowering has them.
- **Mirror refinements** (4): hand-written `Mirror.Product { type MirroredType = [T] =>> … }`
  (curried-mirror, i13332, i13332anonprod, ordinal-innerclass): type members plus HK aliases and
  `Mirror` synthesis, #43.

### 2.9 Singles (adaptation, mostly out)

weak-conformance (`List(b, 33, 5.5)` with `inline val b`: harmonization of an inline constant,
`apply.rs:133-135` refuses other harmonizations as "numeric harmonization"), i14587.min
(`(1, ())` against a union of tuples: the expected type's first alternative is pushed into the
tuple), array-erasure (`case x: Array[_]` then `x(0) = 2`: dotty's pattern type is
`Array[Int] & Array[?]`), i16065 (`Consumer1[? >: String]`, wildcard capture as §2.4),
t3452f (`locally(classOf[Set[_]])`), nats (`new { def add … }` with the parent from the expected
type, `D/Desugar.scala` `AnonClass` of `Object`), collective-extensions (a sibling extension
called unqualified inside a collective `extension (x: …)` block), inline-param-semantics
(inline `Int*`), i7960 (`Future.sequence` `BuildFrom`), tcpoly_parseridioms (`curry2(Plus)` with a
case-class companion as a function, `adaptNoArgsOther`'s `caseCompanion` `T/Typer.scala:4831-4851`),
i9530 (dependent `s.Value` through an extension's `using`). Each ~1 test; collective-extensions
(~20 lines, `Y/extension_calls.rs`) and the case companion (~20 lines, `adapt.rs:55`) are cheap [I].

## 3. MRs

Each gated by `just gates`, `just typer-compare` on the repros that become golden inputs
(`crates/typer/tests/typed`), `just dotty-lower-report`, `just snapshot-diff programs lower typer`
and `just typer-stress` where member lookup changes. Sizes are Rust lines of diff [I].

| MR | scope | lines | tests |
|---|---|---:|---|
| N1 package clauses | §2.1 index nested package clauses first | ~30 | i2964e, i3000b, t5105, t1987 (+Meter, MeterCaseClass after #88; i13332shapeless after #43) |
| N2 apply retries | §2.5 tupled dual prototype; §2.7 view on the qualifier of a failed application; §2.9 case companion as function | ~140 | colltest1, t4813, t5045, OrderingTest, comparable-comparator, richs, tcpoly_parseridioms (+collections) |
| N3 inherited results | §2.6 `asSeenFrom` of the inherited result type | ~30 | t1939, t6070, typeclass-derivation2d (+absoverride after `abstract override`) |
| N4 refined types | §2.2 `Refined` member lookup; §2.8 `typed_refined_type` for type members | ~200 | fully-abstract-nat-5/6/7, i18315, byname-implicits-6/17 (+i23409 after G1; deriving-constructor-order, i7049, poly-kinded-derives after #43) |
| N5 Java Object | §2.7 `FromJavaObject` rule for `java.*` members | ~50 | i8977, i9439 |
| N6 tuples and views | §2.3 `*:` on the `EmptyTuple` alias, small generic tuples cast to `TupleN`; §2.4 wildcard capture in views, `given` selector importing implicits | ~180 | i13968, tuple-product, tuples-empty, tuples1, tuples1a, genericTupleMembers, i14215, array-addition, i23179, i10889 |

Order: N1 → N3 → N2 → N5 → N6 → N4, about 630 lines for ~30 of the 92 run tests; the rest wait
on match-type reduction (the "Tuple" reason, 6 + 15 tests, worth its own research note), #82
(i8577a–i), #43 (mirrors), reflective calls in the lowering (structural types), and JDK-only API.
N1 and N3 touch the namer only and go first; N2 and N6 both extend `select.rs`/`apply.rs` and
should not run in parallel; N4 changes member lookup for every `Refined` type the library has
(`Mirror.Of`, `Selectable` refinements) and goes alone with `typer-stress`.

Risks: N1 moves symbol creation of later clauses before earlier statements are typed, which
changes the order of `unsupported` reports and may surface double-definition checks across
clauses (`Yn/double_definitions.rs`) [I]; N2's tupled retry must not apply to unary methods
(dotty warns and auto-tuples instead, `isUnary`, `Feature.autoTuplingEnabled`) and must keep
`InfixTuple` errors' wording (`T/Applications.scala:931-937`); N4's refinement class needs an
owner for `nat.type`-dependent bounds that the lowering never sees (dotty keeps the class symbol
only as the refinements' owner) [I]; N5 must stay confined to Java-defined members, or
`val o: Object = 1` (an error in dotty, `a04`) would type.
