# 28 — Adaptation, member not found, implicits

Date: 2026-10-20. Probes the three clusters [research 27](27-typer-groups-after-23.md) §2.4–2.6
left "probe first". Claims are [V] verified by reading the code or running it, [I] inferred.
Paths as in research 27: `T/` = `ref/scala3/compiler/src/dotty/tools/dotc/typer/`, `C/` =
`.../dotc/core/`, `Y/` = `crates/typer/src/typer/`, `Yn/` = `crates/typer/src/namer/`, `Yi/` =
`crates/typer/src/inlining/`, `S/` = `crates/tasty/src/`. Method: `just typer-dump` on every
listed test (main 27447a08) plus a throwaway stderr probe at the three refusal sites printing the
tree's type and the expected type (`found → pt` below); line numbers are the refusal's.

**Recommendation.** Of the 70 tests, 2 type now (i13433, sip23-valueof; t1987 types and is
lowering's), 20 belong to other issues or MRs (#41, #120, #127, #129, research 27's H, I, L, W; the
image; experimental features), and the other 48 split into about 25 causes. Research 27's MR J (~270 lines) is replaced by
nine smaller MRs (§5): four cheap ones with several tests each (extension extractors 9 tests,
implicit-parameter defaults 4, default getters of dependent parameters 3, harmonization 3), one
subtyping batch (6 singles), two adaptation batches, synthesis (3) and implicit-search singles.

## 1. Attribution

| cluster | tests | pass now | other issues / MRs | this document's MRs |
|---|---:|---:|---:|---:|
| adaptation (§2) | 24 | 0 | 7 | 17 |
| member not found (§3) | 19 (+ t1987, L) | 0 | 8 | 11 |
| implicit search (§4) | 12 + 15 | 2 | 5 | 20 |

## 2. Adaptation (24 tests, `Y/adapt.rs:117`)

`adapt` refuses when `converted` finds no view, `defined_further` is false and `type_mismatch`
declines to report; the probe gives the tree's type and `pt` [V dumps]:

| test | line | found → pt | cause | MR |
|---|---:|---|---|---|
| extension-methods | 71 | `x` → `List[T]` | `x.compareTo(y)` in `ListOrd[T: Ord]`: `extension_select` (`Y/extension_calls.rs:25-50`) commits to the in-scope extension on `List[T]`; dotty's `tryExtensionOrConversion` (`T/Typer.scala:4352-4370`) drops a failing in-scope extension and falls back to `tryImplicit`, which finds `Ord[T]`'s [V site, I fix] | A |
| tcpoly_parseridioms | 104 | `ParserIdioms.this.Plus` → `Function2[?,?,?]` | case companion as a function: `Y/case_companion.rs:20` accepts only an `Ident`, the tree is a `Select` on `this`; dotty's `caseCompanion` (`T/Typer.scala:4831-4838`) tests `tree.symbol` [V] | A |
| i16065 | 26 | `"foo"` → `>: String <: Any` | `a1: Consumer1[? >: String]`, `a1.accept("foo")`: the member's parameter type became the wildcard's bounds; dotty's `argForParam` (`C/Types.scala:2794`) keeps `TypeRef(pre, tparam)` for a bounds argument, so the formal is the parameter seen from `a1`'s type [V dotty, I ours] | A |
| matchable | 21 | — (§4, ambiguous conversions) | `g(x)` with `x: T` unbounded and `g[T <: Matchable]`: dotty's `adaptToSubType` casts to `T & Matchable` (`T/Typer.scala:5039-5044`) [V dotty, I ours] | A |
| t3048 | 7 | `F2.this.cons` → `?0` | a by-name parameter reference against a type variable: `adapt.rs:91` compares the unwidened `TermRef`; dotty tests `tree.tpe.widenExpr` (`T/Typer.scala:4883`); `S/subtype/third.rs:61` widens by-name only against by-name [V site, I fix] | C |
| t3452f | 17 | `CLASSconst` → `?0` | `locally(classOf[Set[_]])`: a class constant has no underlying `Class[T]` for the variable (`S/subtype/refs.rs:92`, `singletons.rs:70`) [I] | C |
| i7960 | 28 | `Future[?3]` → `Awaitable[?0]` | `Await.result(Future.sequence(…))`: both arguments are type variables (`S/subtype/applied.rs:100`, `third.rs:19` `var_above`) [I] | C |
| t3199b | 4 | `Array[?167]` → `Seq[?0] \| Array[>: Nothing <: ?0]` | `asList(Array(1,2,3):_*)`: a variable against a wildcard bounded by a variable [I]; dotty's `typedTyped` for `_*` builds that union (`T/Typer.scala` `translateFromRepeated`) [V] | C |
| mapConserve | 25 | `input.tail` → `List[>: Nothing <: Any]` | `input: List[_]`: wildcard argument against wildcard argument; dotty's `isSubArg` uses `TypeBounds.contains` [I] | C |
| byname-implicits-6 | 34 | `dh` → `Aux[R, dh.Out]` | `dh: DeepHLister[R]` against `DeepHLister[R] { type Out = dh.Out }`: a path's member against the refinement's alias of that same member (`S/subtype/refined.rs`) [I] | C |
| i14587.min | 3 | `1` → `?2` | `(1, ())` against `(String, Unit) \| (Int, Unit)`: `S/subtype/either.rs` is dotty's `sufficientEither` and keeps the first disjunct's constraint `?2 <: String`; dotty's `constrainResult` compares with `necessarySubType` (`T/ProtoTypes.scala:48-53`, `C/TypeComparer.scala:152`), whose `necessaryEither` (`:2132-2164`) reverts to the constraint before the comparison when neither side subsumes the other [V dotty, I ours] | N |
| implied-priority | 138 | `ev` → `?1` | `HigherPriority.inject(ev)` against `E[T] & HigherPriority.Type`: `?1 & Type <: E[T] & Type` commits `?1 <: Type` (opaque), then `ev: E[T]` fails; same `necessaryEither` [I] | N |
| i8073, i8073b | 7 | `CC.this.b` → `a.B` | `case class CC(a: A, b: a.B)`: the default getter of `copy`'s `b` is typed against the parameter's type, whose `a` is the method's parameter; `Yn/defaults.rs:111-121` maps type parameters only. dotty's `defaultParamType`/`expectedDefaultArgType` (`T/Namer.scala:2210-2260`) approximate references to the method's own parameters by wildcards [V] | G |
| i4600b | 9 | `foo.Bar` → `foo.Bar` | `(bar: foo.Bar = foo.Bar(5))`: the same, two `foo`s [V]; then `(implicit s: String = "ok")` needs D | G, D |
| weak-conformance | 7 | `x1` → `List[Double]` | `List(b, 33, 5.5)` inferred without harmonizing; dotty `harmonize`/`harmonizeWith`/`harmonic` (`T/Applications.scala:2897-2973`) widen `Int` constants to the common numeric class; our four sites refuse "numeric harmonization" (`Y/apply.rs:159`, `control.rs:52`, `matches.rs:58`, `tries.rs:60`) [V sites] | M |
| i9530 | 14 | `x` → `s.Expr` | `extension (using s: Scope)(expr: s.Expr)`: the receiver's formal depends on the leading using clause's parameter, unsubstituted — W's "result depending on an implicit-clause parameter" for a parameter type (`Y/extension_calls.rs`, `dependent_results.rs`) [I] | W |
| string-context-implicits-with-conversion | 19 | `Int` → `Lib.Showed` | `showMe"${1: Int}"` with `given [T] => (show: Show[T]) => Conversion[T, Showed]`: a polymorphic conditional `Conversion` given is not a view candidate (`Y/implicits/view_kinds.rs:27`) [I] | P |
| i14150 | 37 | `*:[head.Out, Tuple]` → `nonEmpty.this.Out` | `head.get *: tail.get` with `head: Aux[H, HRes]`: `T` of `*:` instantiated to its bound `Tuple`, not `tail.Out`; a refinement member (`type Out = TRes`) as a constraint bound (cf. `Y/adapt_by_result.rs:69-86` `refinement_result`) [I]; same shape as byname-implicits-17 (§4) | P |
| inline-param-semantics | 44 | `42` → `<repeated>[Int]` | `inline def h2(inline x: Int*)`: an inline argument typed against the raw repeated type; `i1(42)`/`i2(42)` against `Seq[Int]` (lines 45-46) [V] — #82, research 27 I | I |
| deriving | 16 | `A` → `A` | `Mirror.ProductOf[A] { type MirroredElemTypes = Elems }` (implicit probe) — #129 [V] | #129 |
| i10511 | 18 | `Bool.False` → `Not[B]` | match-type result under a GADT-bound `B` — #41 and #120 [V] | — |
| spreads | 12 | `iarr` → `Seq[?7] \| Array[>: Nothing <: ?7]` | `IArray` spread, then `experimental.multiSpreads` (lines 41-44) — out (research 27 §2.10) [V] | — |
| fully-abstract-nat-4 | 146 | `a1` → `CaseNums.this.Nat` | `a1` from `Succ.unapply(nat): SuccOpt` with `get: Nat` of the outer trait: `Numbers.this.Nat` not rebased to `CaseNums` through the inner trait (as-seen-from) [I]; #127's type tests also fail (`TypeTest[Nat, Succ]` probe) | #127 |
| quoted-sematics-1 | 164 | `Function1[?, Any]` → `?` | `Some(identity)`: a polymorphic method eta-expanded against a type variable instantiates its parameter early; macros test (research 26) [V dump, I cause] | — |

## 3. Member not found (19 tests, `Y/select.rs:89-101`)

| test | line | member on | cause | MR |
|---|---:|---|---|---|
| i8577a–i (9) | 11 | `unapply` on `Macro.StrCtx` | `val mac"$x" = 1`: `StringContext(…).mac.unapplySeq` is an *extension* (`extension (inline ctx: StrCtx) inline def unapplySeq(inline input: Int)`); `Y/unapply.rs:98-124` `unapply_fun` chooses `unapply`/`unapplySeq` among members only (`:104-105`) and refuses an extension (`:120-121`); dotty's `trySelectUnapply` (`T/Applications.scala:1793-1862`) tries `unapply` then `unapplySeq`, each through `typedExpr(Select(qual, name), UnapplyFunProto)`, i.e. `typedSelect` with `tryExtensionOrConversion` [V]. The extractor is also `inline` with inline parameters (research 27 I's i8530 shape) [I risk] | X |
| t2849, t7336 | 14, 30 | `totalMemory` on `java.lang.Runtime` | image / VM native (research 27) [V] | — |
| t13033, t8601c | 4 | `Statics`, `IntRef` on `scala.runtime` | absent from the image [V] | — |
| t3346g | 8 | `copy` on `"asdf"` | `implicit def toA[T](t: T)(implicit f: T => Int): A` — a polymorphic view with an implicit clause whose `T => Int` an `implicit def s2i` must satisfy as a function value [I] | P |
| caseclasses | 22 | `unapply` on `Function1[Int, Abs]` | `case Abs(4)` where a local `def Abs(x: Int)` shadows the companion; ours eta-expands the method; dotty's `tryWithProto` rejects a non-extractor result and falls back (`:1823-1827`) [I] | P |
| convertible | 12 | `str` on `Conversion.into[Text]` | `into` types (`C/Types.scala:475` `isInto`), 3.7 feature — out | — |
| i23279 | 26 | `FromZIO` on `defn.Lifecycle` | local objects inside an inline argument (`simpleInlineWrap { … }`) — research 27 I's "class in an inline argument" [I] | I |
| deriving-from-product-typed | 9 | `fromProductTyped` on a `Mirror.Product { … }` refinement | #129 [V] | #129 |
| fully-abstract-nat | 132 | `value` on `Nat` | the view `NatDeco(nat: Nat): AbstractNat` is the method being defined (`implicit def` with a declared result used in its own body) [I]; the `TypeTest[Zero, Succ]` probes are #127 | #127 |
| t1987 | — | — | types now; lowering `L/codegen/expr.rs:114` (research 27 L) [V] | L |

## 4. Implicit search (12 + 15 tests)

"No implicit argument" (`Y/implicits/search.rs:83,120`, `clauses.rs:50`):

| test | line | searched | cause | MR |
|---|---:|---|---|---|
| Meter, MeterCaseClass | 65, 62 | `Meter.MeterArg` | `(implicit dummy: MeterArg = null)`: no given, the default must apply; dotty re-types the call with the found arguments as named arguments and the defaults (`T/Typer.scala:4681-4710`); ours refuses at `clauses.rs:50` [V] | D |
| i4073, i4600 | 12, 10 | `String` | `(implicit s: String = "World")`, the same [V] | D |
| typable | 5 | `Typeable[String]` | the alias `Typeable[T] = TypeTest[Any, T]` is not dealiased before synthesis (`Y/implicits/synthesis.rs:33` matches the class name) [I]; then `!f[Int]` selects `unary_!` on the unapplied implicit method `(using Typeable[Int]): Boolean` (`Y/select.rs:37` `instantiated_selected` instantiates type parameters only) [V probe] | A |
| implied-specifity-2 | 61 | `Bam` | `given lo(using Low)` vs `given hi(using High)(using Arg)`: the ranking of givens by their using clauses [I]; the `.str` symptom is typable's | P |
| implicit-alias | 64 | `TC` | `given t: [X] => TC`: a polymorphic given whose result does not mention its parameter [I] | P |
| tagless | 40 | `Exp[?T]` | `lit(8)` with `(using e: Exp[T]): T` under the variable `pt` of `add`: `Y/adapt_by_result.rs:52-53` refuses a bare-variable result; dotty constrains `?T <: ?l` and lets the contextual `Exp[T]` instantiate it [V site, I fix] | P |
| byname-implicits-17 | 74 | `Generic.Aux[?, ?]` (depth 3) | `showGen[T, R](implicit gen: Generic.Aux[T, R], sr: => Show[R])`: `Aux` refinements with variables in a by-name chain — i14150's shape [I] | P |
| manifest-summoning | 10 | `OptManifest[Array[? <: Int]]`, `Manifest[List[Int]]`, … | `manifestFactoryOf` (`T/Synthesizer.scala:688-796`) [V] | S |
| tupled-function-andThen, -compose | 10, 9 | `TupledFunction[?, Function1[?, ?]]` | `synthesizedTupleFunction` (`T/Synthesizer.scala:128-168`: `scala.runtime.TupledFunctions.tupledFunction$arity`, XXL above 22) [V] | S |
| i13433 | — | — | types now [V] | — |

Singles by text [V sites]:

- ambiguous implicit conversions (`Y/implicits/conversions.rs:81-89`): t8280 (`5: String` with `Conversion[Int, String]` vs `implicit def f2(x: Long)`: ranking a `Conversion` value against a method that applies by widening — `most_specific`/`beats_widened` [I]); matchable (§2, A); i3539 (`i2s.asInstanceOf.asInstanceOf[…]`: a selection on an uninstantiated variable searches views instead of instantiating to `Nothing` [I]).
- numeric harmonization (`Y/control.rs:52`, `apply.rs:159`): puzzle (`if (false) 5.0 else '5'`), i9939 (`nonHarmonized(3.0, 42)` with `A >: Any`) — M with weak-conformance [V].
- implicit candidate outside the subset (`Y/implicits/attempts.rs:49`): instances, instances-anonymous — the nested refusal is "type lambda other than the right-hand side of a type definition" at `given readerMonad: [Ctx] => Monad[[X] =>> Ctx => X]` (line 108) — research 27 H's first item [V probe].
- implicit import from a value other than an object (`Y/implicits/candidates.rs:86-87`): range (`import num.*`, `num: Integral[T]` a class parameter), typelevel-numeric — dotty imports the implicit members of any stable prefix (`ImportInfo.importedImplicits`) [V site, I fix].
- implicit search too deep (`search.rs:16` `MAX_DEPTH = 6`): iterator-from (`to(SortedMap)` factories) — dotty bounds nesting by divergence only [V site].
- i23897 (#129); typealias_overriding (`scope.rs:41`), byname-implicits-28 (`dictionary.rs:89`): singles, left.
- sip23-valueof types now [V].

## 5. MRs

Gates as research 27 §3 (`just gates`, `typer-compare` on the probes that become golden inputs,
`dotty-lower-report`, `snapshot-diff programs lower typer`). Sizes are Rust lines of diff [I].

| MR | scope | files | dotty | lines | tests |
|---|---|---|---|---:|---|
| X extension extractors | `unapply`/`unapplySeq` tried in turn through `typed_select`, an extension application accepted as the extractor; inline extractor expansion | `Y/unapply.rs:98-124`, `Yi/` (inline `UnApply` funs) | `trySelectUnapply` `T/Applications.scala:1793-1862` | ~60 (+ ~60 if the inliner must learn `UnApply` funs) | i8577a–i (9) |
| G default getters | references to the method's value parameters in a default getter's expected type approximated by wildcards | `Yn/defaults.rs:13-37,111-121` | `defaultParamType`, `expectedDefaultArgType` `T/Namer.scala:2210-2260` | ~20 | i8073, i8073b, i4600b (with D) |
| D implicit defaults | a failed implicit clause of a method with defaults re-typed as an application with the found arguments named and the defaults | `Y/implicits/clauses.rs:46-55`, `Y/default_args.rs` | `adaptNoArgsImplicitMethod` `T/Typer.scala:4681-4710` | ~40 | Meter, MeterCaseClass, i4073, i4600, i4600b |
| N necessary constraints | `either` with a `necessary` flag reverting to the pre-constraint when neither side subsumes; `constrain_applied_result`/`compatible` use it | `S/subtype/either.rs`, `mod.rs`, `Y/adapt_by_result.rs:41-67` | `necessaryEither` `C/TypeComparer.scala:2132-2164`, `necessarySubType` `:152`, `T/ProtoTypes.scala:48-53` | ~30 | i14587.min, implied-priority |
| M harmonization | `harmonize`/`harmonizeWith`/`harmonic` on `Int` constants, called from the four refusing sites | new `Y/harmonize.rs`; `Y/apply.rs:159`, `control.rs:52`, `matches.rs:58`, `tries.rs:60` | `T/Applications.scala:2897-2973` | ~80 | weak-conformance, i9939, puzzle |
| C subtyping singles | by-name and class-constant references against variables; variables on both sides of applied arguments and wildcards bounded by variables; wildcard against wildcard; a path against its own member's refinement | `S/subtype/third.rs`, `applied.rs`, `refined.rs`, `refs.rs`; `Y/adapt.rs:91` | `C/TypeComparer.scala` `isSubArgs`, `compareCaptured`, `hasMatchingMember`; `T/Typer.scala:4883` `widenExpr` | ~70 | t3048, t3452f, i7960, t3199b, mapConserve, byname-implicits-6 |
| A adaptation singles | in-scope extension failing falls back to views; case companion through a `Select`; wildcard-bounded formals; `Matchable` cast; alias dealiased before synthesis; implicit clause applied before a selection | `Y/extension_calls.rs:25-50`, `case_companion.rs:20`, `members.rs:113`, `select.rs:37`, `adapt.rs`, `implicits/synthesis.rs:33` | `T/Typer.scala:4352-4370,4831-4838,5039-5044`, `C/Types.scala:2782-2817` | ~80 | extension-methods, tcpoly_parseridioms, i16065, matchable, typable |
| S synthesis | `TupledFunction`, `Manifest`/`OptManifest` | `Y/implicits/synthesis.rs`, new `tupled_functions.rs`, `manifests.rs` | `T/Synthesizer.scala:128-168,688-796` | ~170 | tupled-function-andThen, tupled-function-compose, manifest-summoning |
| P implicit-search singles | imports from stable values; depth limit; polymorphic givens without using clauses; using-clause ranking; variable results under variable `pt`; conversion ranking; `Aux` refinements under variables | `Y/implicits/candidates.rs:86`, `search.rs:16`, `conversions.rs:86`, `view_kinds.rs`, `adapt_by_result.rs:52` | `T/Implicits.scala` `compareAlternatives`, `ImportInfo` | ~170 | range, typelevel-numeric, iterator-from, implicit-alias, implied-specifity-2, tagless, t8280, (string-context-implicits-with-conversion, t3346g, caseclasses, i3539, i14150, byname-implicits-17 [I]) |

Order by tests per line: X (9 for ~60) → G (3 for 20) → D (4 for 40) → C → N → M → A → S → P;
~380 lines for 30 tests in X–A, S and P add ~340 lines for 10 (plus up to 6 inferred singles). Instances and
instances-anonymous go to H, i9530 to W, inline-param-semantics and i23279 to I, t1987 to L.

Conflicts with research 27's MRs [V file lists, I overlap]:

- `Y/unapply.rs`: X after W (skolemized results, extractor shapes) and #127; X's inline extractor
  shares `Yi/` with I1–I2 (i8530) — one coder for the inliner.
- `Y/default_args.rs`, `named_args.rs`: D after W; G edits `Yn/defaults.rs` only.
- `Y/apply.rs`: M's call site after V1; `Y/extension_calls.rs`: A after V2 (`:62` using clauses).
- `Y/select.rs:37`: A shares the file with U's setter selections and #42 — one coder.
- `S/subtype/`: N and C beside #120's `match_types/` — one coder for the subtyper; N changes every
  result-type constraint, so `just typer-stress` on the pos corpus and `typer-compare` on
  `tests/programs` before merging.
- `Y/implicits/`: S and P replace J (`synthesis.rs`); #123 (`search.rs`) and #129 (`mirrors.rs`)
  stay disjoint except P's `search.rs:16`.

Risks: X's inline `unapplySeq` may need the inliner to expand a call sitting in an `UnApply`'s fun
(then ~120 lines, still 9 tests); D must keep dotty's rule that an ambiguous implicit is not
overridden by a default (`T/Typer.scala:4684-4687`); M widens literals in applications,
conditionals, matches and tries at once — dotty warns on lossy widening, `typer-compare` on
warnings must stay green; A's `Matchable` cast changes typed trees of every unbounded `T` passed
to a `Matchable` bound (a `Typed` cast) — `typer-compare` on the matchable probe.
