# 25 — Circe `Codec` derivation (#94)

Date: 2026-10-16. Claims are [V] verified by reading the code or running it, [I] inferred.
Paths: `Ci/` = `ref/circe-0.14.10/core/io/circe/` (sources of `circe-core_3-0.14.10-sources.jar`,
fetched with `coursier fetch --sources`), `T/` = `ref/scala3/compiler/src/dotty/tools/dotc/typer/`,
`N/` = `.../dotc/inlines/`, `Qd/` = `.../dotc/quoted/`, `Qi/` = `ref/scala3/compiler/src/scala/quoted/runtime/impl/`,
`Y/` = `crates/typer/src/typer/`, `Yi/` = `crates/typer/src/inlining/`. Follows
[research 23](23-typer-groups-after-22.md) §2.1 (mirrors, M1/M2) and [research 12](12-inlining.md).
Direction: no library-specific intrinsic expansion of Circe's macros (ADR 0023 stays for dotc's own
intrinsics); the path is #43 mirrors + `derives`, then the part of phase 4 (#40, an own
`scala.quoted.Quotes` running macro implementations from NIR) that Circe's macros need.

**Recommendation.** Four milestones (§4): M1 product mirrors (#43, ~450 lines), M2 `derives`
(#43, ~150), M3 the smaller typer fixes the example hits on the way (~120), M4 the Quotes
interpreter sized by Circe (#40, ~2,000–2,500: splice interpreter, quote unpickling with holes,
quote-pattern matcher for typed holes, a reflect subset of ~20 methods, the VM bridge, re-inlining
of the expansion). Nothing Circe-specific goes into the frontend; the same four unblock every
Mirror + macro derivation (upickle, zio-json, tapir) as far as their reflect use fits §2.4.

## 1. First refusals on main (`just repl --dep …`)

`just repl --dep io.circe::circe-parser::0.14.10 --dep io.circe::circe-generic::0.14.10` with
the issue's four inputs on stdin (`tmp/r25/repl-input.txt`) [V]:

| input | refusal | site |
|---|---|---|
| `import io.circe.Codec, io.circe.syntax.*, io.circe.parser.decode` | import of a missing member | `Y/import_tree.rs:42-43` — `decode` is inherited by `package object parser extends Parser` (`ref/circe-0.14.10/parser/io/circe/parser/package.scala:21`, `Ci/Parser.scala:45`); `type_members(qual, name)` misses inherited members of a package object [V site, I cause] |
| `case class Person(…) derives Codec.AsObject` | derives clause | `Y/classes.rs:46-47` [V] |
| `Person(1, "a").asJson.noSpaces` | constructor proxy | consequence: `Person` was never defined [V] |
| `decode[Person](…)` | constructor proxy | same [V] |

Probes around it (`tmp/r25/p-*.txt`) [V]:

1. `import io.circe.syntax.*` passes; `1.asJson.noSpaces` and `summon[io.circe.Encoder[Int]]`
   stop with "ValueOf of an undetermined type" (`Y/implicits/synthesis.rs:42-43`): the search for
   `Encoder[Int]` tries `implicit def encodeLiteralInt[L <: Int](implicit L: ValueOf[L])`
   (`Ci/LiteralEncoders.scala`) before `encodeInt`; dotty fails that candidate (`ValueOf` of a
   non-singleton is not synthesized, `T/Synthesizer.scala:236`) and goes on, ours refuses the
   whole search. Fix: return `Ok(None)` for an undetermined or non-singleton argument so the
   candidate fails as in dotty (~10 lines) — needed for *every* `Encoder[Int]`/`Decoder[Int]`
   summon, so for M4's `summonEncoders` as well.
2. `io.circe.Json.fromInt(1).noSpaces` runs (`"1"`); so does a hand-written
   `Encoder.AsObject.instance(p => JsonObject(…))` applied to a case class (`{"id":1,"name":"a"}`):
   the Circe NIR links and runs in the VM, `JsonObject`, `Json`, `Printer` included.
3. `Decoder.forProduct2("id", "name")(P.apply)` stops at "type variable maximized before an
   implicit search" (`Y/param_vars.rs:32-33`): `forProduct2[A0, A1, A](…)(f: (A0, A1) => A)(implicit
   Decoder[A0], Decoder[A1])` — the second clause's variables are constrained from above by
   `P.apply`'s parameter types when the implicit search starts; dotty instantiates them
   (`Inferencing.interpolateTypeVars`), we refuse. Research 23 §2.9 lists this file; ~30 lines.
4. `ConfiguredCodec.derived[P]` with a `given Configuration` stops at "macro call"
   (`Y/inline_refs.rs:28-29`) before the missing `Mirror.Of[P]` is noticed [V].

Scripts: `//> using dep` makes a script take scala-cli (`crates/scivolo/src/frontend.rs:63-64`,
until #118); `scivolo --dep … file.scala` runs through the own frontend and hits the same refusals
[V for the flag, `crates/scivolo/src/main.rs:29-33`].

Libraries [V]: Circe 0.14.10 publishes `_native0.5_3` artifacts (core, generic, parser, numbers,
jawn; cats-core/kernel and jawn-parser come with them, built against Scala Native 0.5.5 and Scala
3.3.3); `libs::deps::fetch` (`crates/libs/src/deps.rs:25-54`) fetches them minus `org.scala-lang`
and `org.scala-native` and warns on jars without NIR — none here. `circe-generic` is not needed
for `derives Codec.AsObject`: the derivation lives in `circe-core` (`Ci/Derivation.scala:157-164`,
`Ci/derivation/`); `circe-generic` holds only `semiauto`/`auto` wrappers.

## 2. The derivation path

### 2.1 What dotty expands [V by reading]

1. `derives Codec.AsObject` → `Deriving.scala` (`T/Deriving.scala:78-104`, `:268-316`) adds to
   `Person`'s companion `given derived$AsObject: Codec.AsObject[Person] = Codec.AsObject.derived`
   (`nme.derived` on the companion of the type class, `:292-303`; the type class is resolved
   with `typedAheadType(derived, AnyTypeConstructorProto)` — a path `Codec.AsObject` whose object
   has a type member of the same name, `underlyingClassRef`). `Codec.AsObject` is an object
   extending `CodecDerivation` (`Ci/Codec.scala:116`).
2. `Codec.AsObject.derived[Person]` is `inline final def derived[A](using inline A: Mirror.Of[A])
   = ConfiguredCodec.derived[A](using Configuration.default)` (`Ci/Derivation.scala:158-159`).
   The `using inline A: Mirror.Of[Person]` is synthesized by `Synthesizer.productMirror`
   (research 23 §2.1): the companion `Person` cast to `Mirror.ProductOf[Person] { type
   MirroredMonoType = Person; type MirroredType = Person; type MirroredLabel = "Person"; type
   MirroredElemTypes = (Int, String); type MirroredElemLabels = ("id", "name") }`.
3. `ConfiguredCodec.derived[A](using conf: Configuration, inline mirror: Mirror.Of[A]) = ${
   derivedImpl[A]('conf, 'mirror) }` (`Ci/derivation/ConfiguredCodec.scala:143-144`): a macro
   (`INLINE | MACRO`). dotty's `Inliner.expandMacro` (`N/Inliner.scala:1263-1288`) →
   `Splicer.splice` (`transform/Splicer.scala:45`) interprets the call shape
   (`Qd/Interpreter.scala:62-192`: static call of `ConfiguredCodec$.derivedImpl` with `Type.of[A]`
   → `TypeImpl`, `'conf`/`'mirror` → `ExprImpl` of the argument trees, `Quotes` → `QuotesImpl`)
   and runs `derivedImpl` as compiled code.
4. `derivedImpl` (`:100-141`) matches `mirror` against the quote pattern `'{ $m:
   Mirror.ProductOf[A] { type MirroredLabel = l; type MirroredElemLabels = el; type
   MirroredElemTypes = et } }` and returns the quote `'{ ConfiguredCodec.ofProduct[A](constValue[l
   & String], summonDecoders[et & Tuple](false)(using $conf), summonEncoders[et &
   Tuple](false)(using $conf), summonLabels[el & Tuple], $m.fromProduct)(using $conf,
   summonInline[Default[A]]) }`. In NIR (`ConfiguredCodec$.nir` of the native jar) the pattern is
   `QuoteMatching.ExprMatch.unapply(scrutinee)(using patternExpr)` and both quotes are
   `QuoteUnpickler.unpickleExprV2(pickledTasty, types, termHole)` calls [V: the names occur in
   `tmp/r25/nir/io/circe/derivation/ConfiguredCodec$.nir`; `Qi/QuotesImpl.scala:3625-3642`].
5. The expansion is inlined again (dotty: `Inlined` result re-entered by `transform/Inlining.scala:162`):
   - `constValue[l & String]` with `l := "Person"` → `"Person"` (`Yi/compiletime.rs:40-45`
     exists; the `&` with `String` must simplify to the constant — dotty's `constToLiteral`,
     `N/Inliner.scala:1008,1015`) [I for the `&`].
   - `summonDecoders[(Int, String) & Tuple](false)` (`Ci/derivation/package.scala:55-60`): an
     `inline erasedValue[T] match` over `_: EmptyTuple` / `_: (t *: ts)` — `Yi/reduce/` exists
     [V]; the scrutinee type is an intersection [I: to probe]. Each element →
     `summonDecoder[t](false)` = `summonFrom { case decodeA: Decoder[t] => decodeA; case m:
     Mirror.Of[t] => … }` (`:66-72`; `Y/summon_from.rs`, `Yi/reduce/patterns.rs` exist). The
     second case's `typeName[A]` macro (`Ci/derivation/TypeName.scala:23`, `Expr(Type.show[A])`)
     is not reached for `Int`/`String`.
   - `summonLabels[("id", "name") & Tuple]` (`:23-26`) → `constValue[t]` per label.
   - `summonInline[Default[Person]]` finds `transparent inline given mkDefault[T](using inline
     mirror: Mirror.Of[T]) = ${ mkDefaultImpl[T]('mirror) }` (`Ci/derivation/Default.scala:60-61`,
     in `Default`'s companion, so in the implicit scope): a second macro. `mkDefaultImpl`
     (`:41-58`) matches the mirror against `'{ ${_}: Mirror.Of[T] { type MirroredElemLabels = el;
     type MirroredElemTypes = et } }` and returns `'{ val size = constValue[Tuple.Size[el &
     Tuple]]; Default.of(getDefaults[T](size).asInstanceOf[Tuple.Map[et & Tuple, Option]]) }`.
   - Its re-inlining: `Tuple.Size[("id","name") & Tuple]` is a match type (#39, research 21,
     `S[…]` through `Yi/type_eval.rs`) → `2`; `getDefaults[T](inline s: Int) = ${
     getDefaultsImpl[T]('s) }` (`:63`) is the third macro: `s.asTerm.underlying` must be the
     `Literal(2)` — dotty's inliner binds an `inline` parameter whose argument has a constant type
     to the literal (`N/Inliner.scala:293` `paramBindingDef`, `constToLiteral`) [I: ours,
     `Yi/proxies.rs`, to check]. `getDefaultsImpl` (`:65-84`) reads
     `TypeRepr.of[T].typeSymbol.companionModule.declaredMethod("$lessinit$greater$default$i")` for
     `i` in `1..n`, builds `Ref(companion).select(sym)` (with `appliedToTypes(typeArgs)` for
     generic `T`), `Expr(None)` for the missing ones, and `Expr.ofTupleFromSeq(exprs)` — for
     `Person` (no defaults) `(None, None)`; `Tuple.Map[(Int, String) & Tuple, Option]` reduces
     to `(Option[Int], Option[String])` (#39).
   - `$m.fromProduct`: `Person.fromProduct` from the lowering's `addMirrorSupport`
     (`crates/lower/src/phases/mirrors.rs`, research 23 §2.1) [V exists].
6. Runtime: `ConfiguredCodec.ofProduct` (`:60-78`), `SumOrProduct`, `ConfiguredDecoder.decodeProduct`,
   `Default.defaultAt` — plain NIR; the hand-written probe (§1.2) shows that layer works. The
   `private def ofProduct`/`Default.of` referenced from quotes are ordinary NIR methods, callable
   from the expansion [I: the lowering emits the call by name; dotty 3.3 adds no accessor].

### 2.2 The three macros and their `Quotes` use [V by reading]

| macro | pattern side (`QuoteMatcher`) | quote side (`unpickleExprV2`) | reflect API |
|---|---|---|---|
| `ConfiguredCodec.derivedImpl[A: Type](conf: Expr[Configuration], mirror: Expr[Mirror.Of[A]])` (and the identical `ConfiguredEncoder`/`ConfiguredDecoder.derivedImpl`, `Ci/derivation/ConfiguredEncoder.scala:98-137`, `ConfiguredDecoder.scala:249-290`) | typed hole `$m: Mirror.ProductOf[A] { type … = l/el/et }` with three *type bindings* (`patternType`, `QuotePatterns.scala:174-184`, `:321-329`) matched by `isSubTypeUnderEnv(scrutinee, tpt)` (`Qi/QuoteMatcher.scala:274-279`): a subtype check of the mirror's refined type against the pattern type with `l`, `el`, `et` as type variables; second case `Mirror.SumOf[A]` the same | one quote, type holes `A`, `l`, `el`, `et` (`types: Seq[Type[?]]`), term holes `$conf` (twice), `$m` (`termHole(idx, args, quotes)`) | none beyond `Type[A]` and `Expr`; `import q.reflect.*` is unused |
| `Default.mkDefaultImpl[T: Type](mirror: Expr[Mirror.Of[T]])` (`Default.scala:41-58`) | typed wildcard hole `${_}: Mirror.Of[T] { type MirroredElemLabels = el; type MirroredElemTypes = et }`, two type bindings | one quote, type holes `T`, `el`, `et`; no term holes (`$mirror` is not used) | none |
| `Default.getDefaultsImpl[T](s: Expr[Int])(using Quotes, Type[T])` (`:65-84`) | — | `'{ Some(${ et.asExpr }) }` per present default (not for `Person`); `Expr(None)` through `ToExpr` (library code, itself a quote `'{ None }`); `Expr.ofTupleFromSeq` (library: `'{ Tuple2($x1, $x2) }` quotes with term holes) | `Expr.asTerm`, `Term.underlying`, `Literal.constant.value`, `TypeRepr.of[T]`, `TypeRepr.typeSymbol`, `Symbol.companionModule`, `Symbol.declaredMethod(name)`, `Ref(sym)`, `Term.select(sym)`, `TypeRepr.typeArgs`, `Term.appliedToTypes`, `Term.asExpr` |
| `typeNameImpl[A: Type]` (`TypeName.scala:21`, error path only) | — | `Expr(String)` (`ToExpr`) | `Type.show[A]` |

So the Circe-sized subset of `QuotesImpl` (3,651 lines) is: `unpickleExprV2`/`unpickleTypeV2`
(`PickledQuotes.unpickleTerm`, `spliceTypes`, `spliceTerms`, `Qd/PickledQuotes.scala:85-160`),
`ExprMatch.unapply` for patterns made of a single typed hole with type bindings (`treeMatch`,
`Qi/QuoteMatcher.scala:133`, cases `:265-279`, `:317-323`; no higher-order holes, no
`TypeMatch`, no `'[t]` patterns), `Type.of` (`unpickleTypeV2` of a pickled type tree), the
fourteen reflect members of the table, `Expr.asTerm`/`asExpr` wrappers (`Qi/ExprImpl.scala`,
`TypeImpl.scala`) and `report` (errors as compile errors, `StopMacroExpansion`).

### 2.3 Where the own frontend stops, in order [V sites]

1. `derives` (`Y/classes.rs:46`) — M2.
2. `summon[Mirror.ProductOf[P]]`: "no implicit argument" (`Y/implicits/search.rs`;
   `synthesis.rs:22-31` has `CanEqual`, `ClassTag`, `ValueOf`, tuple mirrors only) — M1.
3. The macro reference (`Y/inline_refs.rs:28-29`): `derived`'s body is itself inline-expanded
   from TASTy (`Yi/inline_call.rs`); the inner `ConfiguredCodec.derived` call carries `MACRO` and
   is refused when its reference is checked. dotty's `expandMacro` runs where the inliner meets a
   top-level splice as the inlined body (`N/Inliner.scala:1263`) — M4.
4. The TASTy reader skips `QUOTE`, `SPLICE`, `HOLE`, `QUOTEPATTERN`, `SPLICEPATTERN`
   (`crates/typer/src/unpickle/terms.rs:88`) — the pickled quotes are *strings* in NIR, not
   TASTy trees of the library, so this matters only for the splice body `${ derivedImpl[A]('conf,
   'mirror) }` read from `ConfiguredCodec.tasty` — M4.
5. The items of §1: `ValueOf` candidate failure, inherited members of package objects in
   imports, maximized variables before an implicit search — M3.

### 2.4 What a general Quotes interpreter needs here (M4 scope) [I, sized from §2.1–2.2]

1. **Splice interpreter** (`Qd/Interpreter.scala`, 392 lines; `Splicer.scala:45-87`, `:248-262`):
   the inlined body is `${ f[T](args) }` — a static call whose arguments are `'x` quotes of
   inline-proxy references, `Type.of[T]`, literals and the `Quotes` instance. Ours: ~150 lines in
   `Yi/splice.rs`: build the VM arguments (`ExprImpl`/`TypeImpl` host objects holding a `TNodeId`
   / `TypeId`), call the method through the VM, take the returned `Expr` back to a tree.
2. **VM bridge**: `scala.quoted.Quotes` is an abstract class the macro receives; ours is a VM
   class implemented by natives that call back into the typer (`crates/vm/src/natives/`, the
   `callback.rs` slot table of ADR 0002 is the model). The typer must own or borrow a running
   `Vm` during inlining: the REPL has one (`crates/repl`), scripts create theirs after the
   frontend (`crates/scivolo/src/main.rs:111-127`) — the script path needs the VM created first,
   or a second VM for macros (an ADR). ~400 lines, the largest unknown.
3. **Quote unpickling**: `unpickleExprV2(pickled: String | List[String], types, termHole)` — the
   pickled quote is TASTy bytes (`Qd/TastyString.scala`) read with `crates/tasty` into our typed
   trees (`crates/typer/src/unpickle/` reads library bodies already), then `spliceTypes` (type
   holes → the `Type[?]` arguments' types) and `spliceTerms` (`Hole` nodes → `termHole(idx,
   args)` results) and owner/span fixing (`Qd/PickledQuotes.scala:102-160`). ~300 lines; `HOLE`
   in `unpickle/terms.rs:88` becomes a node.
4. **Quote-pattern matcher**: `ExprMatch.unapply(scrutinee)(pattern)`: the pattern is itself an
   unpickled quote with `patternHole[T]` calls and `patternType`-annotated type bindings
   (`Qd/QuotePatterns.scala:145-184`); the `Typed(TypeApply(patternHole, tpt), …)` case is a
   subtype test of the scrutinee's type against `tpt` with the bindings as type variables, the
   result a tuple of `Type[?]`s (the instantiated bindings) and `Expr[?]`s (the matched terms) —
   `Qi/QuoteMatcher.scala:133-195`, `:261-279`. Ours: ~250 lines over `crates/tasty/src/subtype`
   with the typer's constraint for the bindings; structural matching of other tree shapes
   (`:209-`) is the long tail other libraries need — Circe needs only the hole cases.
5. **Reflect subset**: the fourteen members of §2.2 over `Universe`/`tpd` (~250 lines), plus
   `Type.show` (printer, there is one for the REPL, ADR 0014/`:type`). Everything else of
   `QuotesImpl` is refused at the native with the member's name ("macro uses `Symbol.tree`"),
   so the script falls back as today.
6. **Re-inlining of the expansion**: the returned tree is typed already; its inline calls
   (`constValue`, `summonDecoders`, `summonInline`, `getDefaults`) go through `Yi/expand.rs`
   again, as the `Inlining` phase re-traverses dotty's expansion — the `Inlined(call, …)` wrapper
   and spans as dotty's for `just typer-compare` (the oracle expands macros on the JVM, so
   `tests/pos` gains a comparable corpus: #40's `quotes`/`macro_calls` files). ~100 lines plus
   the inline-parameter constant binding and `l & String` simplification of §2.1.5 (~40).
7. **Errors**: `report.errorAndAbort` → a compile error at the splice (`StopMacroExpansion`);
   `error("Failed to find an instance of Encoder[…]")` already goes through `Yi/compiletime.rs:157`.

Not needed by Circe, stays out: `'[t]` type patterns and `TypeMatch`, higher-order holes,
`Symbol.tree`/`Term` construction beyond `Ref`/`select`/`appliedToTypes`, `Expr.summon`
(`Implicits.search` from a macro — plausible next client: zio-json uses it), `Quotes` at
level ≥ 2 (nested quotes in the *script*: `QuotesAndSplices.scala`, 394 lines, is the typer side
of writing macros, not of calling them).

## 3. Gaps, in order

| # | gap | layer | size | issue |
|---|---|---|---|---|
| 1 | product (and sum) mirror synthesis: `Y/implicits/mirrors.rs` (`productMirror`, `MirroredElemTypes/Labels`, companion ref cast to the refined `Mirror.ProductOf`), `MirroredElemTypes` lookup on a `using` parameter | typer | ~450 | #43 (research 23 M1) |
| 2 | `derives`: `Deriving.scala` → `given derived$TC` in the companion, `typedAheadType` of a path type class (`Codec.AsObject`), `hasExplicitParams` check | typer | ~150 | #43 (M2) |
| 3 | `ValueOf` candidate failure instead of refusal (`synthesis.rs:42`); inherited members of package objects in named imports (`import_tree.rs:42`); maximized variables before an implicit search (`param_vars.rs:32`) | typer | ~10 + ~40 + ~30 | #94 (new: #94-M3) |
| 4 | splice interpreter + VM bridge (`Quotes` as a native class, typer ↔ VM in one process, script path creates the VM before compiling) | typer, VM | ~550 | #40 |
| 5 | quote unpickling with type and term holes (`unpickleExprV2`, `unpickleTypeV2`, `HOLE` node) | typer (`unpickle/`) | ~300 | #40 |
| 6 | `ExprMatch.unapply` for typed holes with type bindings | typer | ~250 | #40 |
| 7 | reflect subset of §2.2 (14 members) + `Type.show` | typer, VM natives | ~250 | #40 |
| 8 | re-inlining of the expansion, `Inlined` wrapper, inline-parameter constants, `l & String` | typer (`inlining/`) | ~140 | #40, #82 |
| 9 | match types `Tuple.Size`, `Tuple.Map` over `& Tuple` scrutinees; `erasedValue` match on an intersection | typer | probe first; ~40 if needed | #39 |
| 10 | `fromProduct` of the companion for the mirror's `$m.fromProduct` | lowering | exists (`phases/mirrors.rs`) | #43 |
| 11 | Circe NIR availability: `_native0.5_3` artifacts load, run (`Json`, `Encoder.AsObject.instance`) | libraries | done | #93 |

Not on the path: `circe-generic` (`semiauto.deriveCodec` is `inline def … = Codec.AsObject.derived`
on top of the same macros), ADR 0023 intrinsics (ruled out), the `summonFrom` error branch's
`typeName` macro (only reached on a missing `Encoder`).

## 4. Milestones

| MS | scope | lines | acceptance |
|---|---|---:|---|
| M1 | product/sum mirrors (#43, gap 1) | ~450 | `summon[Mirror.ProductOf[P]]` in the REPL prints the companion with its refined type; research 23's 17 `summon[Mirror…]` tests |
| M2 | `derives` (#43, gap 2) | ~150 | `case class P(…) derives CanEqual` and a user `derives Show` with `inline def derived` in the REPL; the six `derives` tests of research 23 |
| M3 | the three small fixes (gap 3) | ~80 | `1.asJson.noSpaces`, `import io.circe.parser.decode`, `Decoder.forProduct2("id","name")(P.apply)` run; `tests/repl/circe.check` with the hand-written codec of §1.2 |
| M4a | Quotes core (gaps 4, 5, 8; an ADR for the typer ↔ VM bridge) | ~1,000 | a macro of the form `${ impl('x) }` whose impl returns `'{ … $x … }` runs from a `--dep` jar and from `tests/pos` (`macro_calls`), with `just typer-compare` against dotty's expansion |
| M4b | quote patterns with typed holes + reflect subset (gaps 6, 7) | ~500 | `ConfiguredEncoder.derived[P]` (one macro, no `Default`) encodes `P(1,"a")`; then `Codec.AsObject.derived[P]` with `Default` |
| — | integration (#94) | — | the issue's four inputs through `just repl --dep …` print `{"id":1,"name":"a"}` and `Right(Person(1,a))`; the same file through `scivolo --dep … circe.scala`; `tests/repl/circe-derives.check` skipped when the Coursier cache lacks Circe (as `crates/repl/tests/*.rs:50-72` do) |

Order: M3 (independent, one day) → M1 → M2 → M4a → M4b. M1+M2 is research 23's plan unchanged
and worth doing first because the mirror is the macro's *input*: M4 cannot be tested on Circe
without it, and the mirror tests are JVM-free. M4a lands behind a flag until M4b passes
`typer-compare` on the pos corpus's macro files.

M4a findings (2026-10-16, ADR 0029) [V, `tests/macros/lib`]: a macro body in TASTy is
`TYPED(SPLICE(BLOCK(DEFDEF $anonfun(contextual$: Quotes) = impl(QUOTE(x).apply(q))(q), LAMBDA)))`;
a pickled quote (Base64 TASTy, the tooling's version) is `INLINED(expr, call = the macro's
module)`, `expr` a `BLOCK` of `@SplicedType` `TYPEDEF`s whose right-hand side is a type `HOLE`
when it has type holes, each term hole `INLINED(HOLE idx tpe args*)`; `'{ 42 }` compiles to
`ToExpr.IntToExpr`, i.e. `quotes.reflect` (so M4b); the hole closure of a quote can be filled
with an expression made before the quote (`typed('{ … })` with a `Type.of` from
`unpickleTypeV2`). Quote locals (`'{ val v = $x; … }`) reach the lowering's refusal of
definitions from library inline bodies (`phases/drop_inlined.rs`), as plain library inline
bodies with locals do.

M4b findings (2026-10-19) [V, `tests/repl/deps/circe-derives.check`, `crates/scivolo/tests/circe.rs`]:
a quote pattern pickled by 3.3.3 is `INLINED(BLOCK(BLOCK(TYPEAPPLY(patternHole, T), @patternType
TYPEDEF l/el/et), @SplicedType TYPEDEF A = HOLE))`, without a `TYPED`; `${_}` is a `patternHole`
too, its match returned like `$m`'s. The open [I] items, resolved: `constValue[l & String]` needs
the `&` simplified (`l <: String`); `erasedValue[(Int, String) & Tuple]` matched against `_: (t *:
ts)` needs inline-match type bindings in library bodies (bindings of `BIND` in pattern type trees,
instantiated by a subtype check under a constraint, as quote patterns are) and `summonFrom` read
from TASTy; `Tuple.Size[("id", "name") & Tuple]` reduces through the existing match type reduction
once `constValue` normalizes; `getDefaults[T](size)` sees `Literal(2)` because the splice turns a
quoted reference to a value whose type normalizes to a constant into the literal. Also needed:
`summonInline` results with their inline calls expanded (`Default.mkDefault` is a transparent
inline given), a macro's own body not counting as a local implicit, wildcard type arguments
(`TYPEBOUNDStpt`) in library type trees, and the lowering no longer refusing definitions from
library inline bodies (they take the call's position).

Risks: the VM bridge (gap 4) is the first time the typer runs code; the REPL's VM is
single-threaded and already in the interpreter thread (`main.rs:70-76`) — the typer calls into it
synchronously, GC roots for host-held `Expr` objects need a handle table (ADR 0002's slots).
Pickled quotes are TASTy of Scala 3.3.3 (the native artifacts' compiler) — the reader accepts
older minor versions for libraries already [I]. `just typer-compare` cannot cover `--dep` macro
expansions (the oracle has no Circe on its classpath); the REPL transcript test is the oracle there.
