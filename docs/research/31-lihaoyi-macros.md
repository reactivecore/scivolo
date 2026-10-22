# 31 — fastparse, upickle and mainargs macros (#144, #40)

Date: 2026-10-22. Probes of research 30's three large macro steps FP, UP, MA on main c8c1b5f0
(T0, M1, P0, F1, O1, A1, L1 landed; S1, E, X1, I1 in flight, assumed to land as research 30
§2.6/§2.8 describe). Claims are [V] verified by running or reading, [I] inferred. Paths: `Y/` =
`crates/typer/src/typer/`, `Yi/` = `crates/typer/src/inlining/`, `Yu/` =
`crates/typer/src/unpickle/`, `M/` = `crates/macros/src/`, `O/` =
`overlay/src/scala/quoted/runtime/impl/`, `Qi/` = `ref/scala3/compiler/src/scala/quoted/runtime/impl/`,
`L/` = the Scala 3.9.0 library sources (`scala-library-3.9.0-sources.jar`, `tmp/sources/scala3-library/`),
`Fp/` = fastparse 3.1.1 `fastparse/internal/` (`fastparse_3-3.1.1-sources.jar`), `Up/` =
upickle 4.4.3 `upickle/implicits/` (`upickle-implicits_3-4.4.3-sources.jar`; `Readers.scala`/`Writers.scala`
of `src-3` are missing from that jar and were read from the tag on GitHub), `Ma/` = mainargs 0.7.8
(`mainargs_3-0.7.8-sources.jar`). Probes: `tmp/eval-lihaoyi/dep/<name>.scala` through
`SCIVOLO_FRONTEND=strict scivolo --dep COORD FILE` and `tmp/eval-lihaoyi/<name>/Main.scala` through
`just aot-src` (a `//> using dep` directive still refuses the script in the interpreter, so the
`--dep` copies carry none); both backends gave the same result for every probe [V].

**Recommendation.** Every macro of the three libraries now reaches the macro host (§1): mainargs and
fastparse's `LiteralStr` fail on the first missing `reflect` member, upickle and the rest of
fastparse on four small typer gaps (§2, ~60 lines, independent of S1/E — do them now). The
`reflect` work is eight MRs of ≤ 300 lines (§4), by slice rather than by library, since mainargs
needs a subset of upickle's symbol and type slices. Order: FP0 → R1 → R2 (fastparse passes, ~330
reflect lines: the best value per line) → R4 → R3 → R5 → R6 (mainargs) → R7 → R8 (upickle product,
then sum). Research 26/30 put `Type.show` (S) on upickle's and mainargs' success paths; reading the
sources, it is on error paths only (§3.2, §3.3), so S is not needed for the three programs.

## 1. Probes

| probe | program | result (first refusal) | § |
|---|---|---|---|
| fp | `P(CharsWhileIn("0-9").!.map(_.toInt))`, `~`, `rep`, `End`, `parse` | "extension method with a using clause after the receiver" at `.!` [V] | 2.1 |
| fp2, fp3 | `num ~ ("+" ~ num).rep(0) ~ End`; `num ~ num ~ End` | "implicit search for a type with type variables" (the `Sequencer[Unit, Unit, R]` clause of `~`) [V] | 2.2 |
| fp4 | `num.rep(0)` | "inline argument of type Nothing" (`rep`'s default `sep = null`) [V] | 2.3 |
| fp5 | `P("+")`, `P(LiteralStr("ab"))` | the macro runs: "no method …ReflectImpl::BlockTypeTest" from `FromExpr.PrimitiveFromExpr.unapply` (`L/FromExpr.scala:105`) via `Quotes.value` [V] | 3.1 |
| upk | `case class P(x: Int, name: String = "n") derives ReadWriter`, `sealed trait Shape derives ReadWriter`, `write`/`read` | "selection of an unknown member in a library inline body": `m.MirroredElemTypes`, four times (the `Mirror.SumOf` branches of `macroR`, `macroRAll`, `macroW`, `macroWAll`) [V, a probe print in `Yu/selections.rs:105`]; the member view `ReadWriter.derived` through `ReadWriterExtension` (research 30 §2.10's risk) passes [V] | 2.4 |
| ma | `@main def run(@arg(short = 'f', doc = "…") foo: String, @arg(doc = "…") n: Int = 2)`, `ParserForMethods(this).runOrExit(args)` | the macro runs: "no method …SymbolMethodsImpl::memberMethods", its first `reflect` call (`Ma/Macros.scala:10`) [V] | 3.3 |

## 2. Typer gaps before the macros (FP0 + UP0, ~60 lines)

### 2.1 Extension with only a using clause after the receiver — `Y/extension_calls.rs:70-73`

`extension [T](inline parse0: P[T]) inline def !(using ctx: P[Any])`, `rep[V](using …)`, `?`, `/`
(`Fp/../package.scala:51-57,93,137`): `ext_method_apply` refuses when the type after the receiver
is an implicit method and `pt` is no `Fun` [V shape `tmp/eval-lihaoyi/shapes/ext_using_only.scala`].
dotty applies the using clause in `adapt` (`adaptNoArgsImplicitMethod`). Fix: take the
`with_using_args` path of line 77 for any non-`Fun` `pt`. ~10 lines.

### 2.2 Several fitting candidates under type variables — `Y/implicits/search.rs:20`

`Sequencer[Unit, Unit, ?R]` fits `SingleSequencer[T]: Sequencer[Unit, T, T]` (`object Sequencer`),
`UnitSequencer[T]` (`LowPriSequencer`) and the generated `Sequencer1`… (`SequencerGen`)
(`Fp/../Implicits.scala:16-32`); dotty ranks them by owner (`compareOwner`, ours
`Y/implicits/ranking.rs:108`) and `SingleSequencer` wins with `R := Unit`. Ours refuses `OPEN`
before ranking when the goal still has variables [I where; V the probe]. `rep`'s
`Repeater[Unit, ?V]` (`UnitRepeater` in `object Repeater` vs `GenericRepeaterImplicit` in
`LowPriRepeater`, `:40-50`) is the same shape — research 30 §2.4's leftover [I]. ~30 lines.

### 2.3 Inline by-name parameter with a `null` default — `Yi/proxies.rs:35-37`

`rep(inline min: Int = 0, inline sep: => P[_] = null, …)` (`Fp/../package.scala:176-180`): the
default's type `Null` is a bottom type and the inline binding is refused. dotty binds the proxy
with the formal's type; the macro then matches `case '{ null }` on it (`Fp/MacroRepImpls.scala:140-141`).
Fix: no refusal for a by-name inline formal; ~5 lines [I].

### 2.4 Refinement type members in library inline bodies — `Yu/selections.rs:33-45,105`

`m.MirroredElemTypes` with `m: Mirror.SumOf[T]` (`Up/Readers.scala:230,240`,
`Up/Writers.scala:65,75`): `select` resolves through `member_of_type`, which descends past the
refinement into `Mirror.Sum` (`crates/tasty/src/symbols/members.rs:40-42`), and `Sum` does not
declare the member — only the alias's refinement does (`L/scala/deriving/Mirror.scala:21-27,52`)
[V]. The typer has `refined_select` for script code (`Y/refined_types.rs:91-106`, over
`Universe::refinement_named`, `crates/tasty/src/types/refinements.rs:24`). Fix: `select` of a
`SELECTtpt` falls back to `refinement_named(prefix, name)` and a `TypeRef(prefix, name)` without a
symbol. ~10 lines. The refusal comes from the branch the inline match does *not* take, since the
unpickler reads the whole body — after the fix, the product path never runs that branch [V].

## 3. What each library's macros need, in call order

What exists today: `Literal.apply/unapply/constant`, `Term.underlying/select/appliedToTypes`,
`Ref(sym)`, `TypeRepr.of/typeSymbol/typeArgs`, `Symbol.companionModule/declaredMethod/owner/name/fullName/spliceOwner`,
`Position` and `SourceFile` (`O/ReflectImpl.scala`, `O/ReflectSymbols.scala`, `M/reflect.rs:20-70`,
`M/positions.rs:18-53`) [V]; typed-hole quote patterns with type bindings and spliced types
(`Yu/quote_patterns.rs`, `Yi/quote_match.rs`), hole arguments as `Expr`s
(`Yu/quotes.rs:131-148`, `O/QuotesImpl.scala:42-45`) [V]. No `report`, `Flags`, `defn`,
`Implicits`, `TypeTree`, `Match`, `Block`, `Repeated` objects at all [V grep]; `isExprOf`,
`asExprOf`, `valueOrAbort`, `TypeMatch` refused by name (`O/QuotesImpl.scala:18-23,79-82`).

### 3.1 fastparse (`Fp/MacroInlineImpls.scala`, `Fp/MacroRepImpls.scala`)

The program's macros: `LiteralStr` → `literalStrMacro` (`:9-76`), `~` → `parsedSequence0`
(`:130-218`), `rep` → `repMacro0` (`MacroRepImpls.scala:19-181`), `CharIn` → `charInMacro` and
`parseCharCls` (`:335-411`), `CharsWhileIn` → `charsWhileInMacro` (`:425-453`); `P` is inline over
`sourcecode.Name` (S1). `StringIn` (`stringInMacro0`, `:468-546`) is outside the program.

| step | operations (dotty `Qi/QuotesImpl.scala`) | ours | slice |
|---|---|---|---|
| `s.value`, `Exprs`, `min.value` | `FromExpr.PrimitiveFromExpr.unapply` (`L/FromExpr.scala:93-106`): `Block`/`Inlined`/`Typed` unapply, then `tree.tpe.widenTermRefByName` and `ConstantType(c)` (`:460`, `:1884`, `:1993-2010`) — the *type* path, not `Literal(IntConstant(x))` as research 26 §1 assumed | `Literal.constant` only | E (TypeTests), **E+**: `Term.tpe`, `widenTermRefByName`, `ConstantType` TypeTest + unapply |
| `Expr(x)` for `String`, `Char`, `Int`, `Boolean`; `Expr(List[String])` | `ToExpr` → `Literal(…)` (`L/ToExpr.scala:28-90`); `'{ List(${Varargs(xs)}*) }` (`:179`) | `Literal.apply` [V]; `Varargs.apply` → `Repeated.apply` (E) | — |
| quotes with holes over the outer quote's locals (`input`, `index`, `ctx1`, `charIn`, `rec`, `end`), `$ctx match { case ctx1 => … }`, `@tailrec def rec` inside a quote | hole arguments (research 26 §3.2) | `Yu/quotes.rs:131-148` [V]; `Match`/`Bind` in quotes `Yu/cases.rs` [I]; an annotated `DefDef` in a quote: no `ANNOTATION` handling in `Yu/` [I risk] | — |
| `ws.asTerm.tpe =:= TypeRepr.of[NoWhitespace.noWhitespaceImplicit.type]` (`:205`, `MacroRepImpls:77`) | `tpe` `:460`, `=:=` `:1881` | `Universe::is_subtype` | **W1-lite**: `tpe`, `=:=`, `<:<`, `widen` |
| `asExprOf[Int]`/`[Boolean]`/`[Unit]` (`MacroRepImpls:38-40`, `:375`, `:527`) | `isExprOf` = `tpe <:< TypeRepr.of[X]` (`:68-80`) | refused `O/QuotesImpl.scala:22-23` | over W1-lite, ~10 Scala |
| `case '{-1}`, `case '{ null }` (`MacroRepImpls:43,47,141`) | `QuoteMatcher` `Literal` case (`Qi/QuoteMatcher.scala:327`) | a pattern without hole or binding unpickles as a quote (`Yu/quote_patterns.rs:30-32`) and `match_pattern` refuses it (`M/host.rs:180-182`) | **P1-lite**: `QuotePattern::Literal`, matched after `Inlined`/`Typed`/empty-`Block` stripping, ~40 Rust |
| `Inlined(a, b, c)`, `Typed(a, b)` (`MacroRepImpls:30-36`), `Varargs.unapply` (`L/Varargs.scala:54-64`: `Repeated`/`Typed`/`Block`/`Inlined`) | `:1064-1115`, `:809-845`, `:882-900` | — | E |
| `CaseDef(Expr(c).asTerm, None, …)`, `Wildcard()`, `Match(scrutinee, cases)` (`:356-375`, `:507-526`) | `:1556`, `:553-565`, `:971-1032` | — | **W4-lite**: `Match.apply`, `CaseDef.apply`, `Wildcard.apply` as `tpd` nodes; `Yi/retype.rs` must type a macro-built `Match` [I risk] |
| `report.errorAndAbort(msg, expr)` (`:409,430`) — error path | `:3435-3446` | none | **W6-lite**: `report` with the three `errorAndAbort`s throwing an exception `M/lib.rs:49` maps to `MacroError::Failed` |
| `valueOrAbort` (`StringIn` only, `:475`) | `:56-66` = `value` or `report.throwError` | refused `O/QuotesImpl.scala:18` | 3 Scala lines over E+ and W6-lite |

### 3.2 upickle (`Up/macros.scala`; entry `derived` → `macroRWAll` → `macroRAll`/`macroWAll`, `Up/MacroImplicits.scala:13-14,49`)

Product path, in order (`Up/Readers.scala:195-226`, `Up/Writers.scala:16-61`):

1. `validateFlattenAnnotation[T]` → `allFields[T]` (`:138-175`) → `fieldLabelsImpl0[T]` (`:177-190`):
   `typeSymbol.primaryConstructor.paramSymss.flatten.filterNot(_.isType)`, `TypeRepr.isSingleton`,
   `extractKey`: `sym.annotations.find(_.tpe =:= TypeRepr.of[key])` matched as
   `Apply(_, Literal(StringConstant(s)) :: Nil)` (`:39-44`); `getDefaultParamsImpl0[T]` (`:7-37`):
   `AppliedType.unapply` on a `TypeRepr`, `isClassDef`, `companionClass`, `caseFields`,
   `flags.is(Flags.HasDefault)`, `comp.tree.asInstanceOf[ClassDef].body`, `DefDef(name, _, _, _)`,
   `Ref(deff.symbol)` (+ `appliedToTypes(typeArgs)` for a generic class), `'{ $e.asInstanceOf[AnyRef] }`;
   per field `extractFlatten` (annotations), `substituteTypeArgs` (`:401-408`: `paramSymss`,
   `isType`, `substituteTypes`, `typeArgs`), `classTypeRepr.memberType(field)`, `typeSymbol`.
2. `paramsCount[T]`, `extractIgnoreUnknownKeys[T]` (`typeSymbol.annotations`, `Expr.ofList`).
3. `applyConstructor[T]` (`:410-481`): `tpe.classSymbol.get.companionModule`, `primaryConstructor`,
   `paramSymss.partition(_.isType)`, `tpe.memberType(ctor).widen`, `memberType(param)`,
   `substituteTypes`, `tpe.asType match { case '[t] => '{ $lhs.asInstanceOf[t] }.asTerm }`,
   `Select.overloaded(Ref(companion), "apply", typeArgs, args)`, `TypeRepr.of[T] match { case t:
   AppliedType | TypeRef | TermRef }`, `asExprOf[T]`; `AnnotatedType(AppliedType(_, Seq(arg)), x)`
   with `x.tpe =:= defn.RepeatedAnnot.typeRef`, `Typed`, `TypeTree.of`, `defn.RepeatedParamClass`
   for varargs fields only.
4. `allReaders[T, Reader]` (`:79-114`): `TypeRepr.of[R].appliedTo(tpe)`, `asType`/`'[t]`,
   `'{ summonInline[t].asInstanceOf[AnyRef] }` (expanded when the result is retyped,
   `Yi/compiletime.rs:56` [I]), `Expr.ofTuple` (→ `ofTupleFromSeq2`, typed-hole patterns, works),
   `Expr.ofList`.
5. `keyToIndex[T]` (`:192-206`): `Match`/`CaseDef`/`Wildcard` with `Literal(StringConstant)` and
   `Literal(IntConstant)` — W4-lite; `allFieldsMappedName[T]` (`Expr(List[String])`);
   `storeDefaults[T]` (`Expr.block` → `Block.apply`, `L/Expr.scala:75-78`).
6. `isSingleton[T]` (`flags.is(Module)`, `isSingleton`), `isMemberOfSealedHierarchy[T]`
   (`baseClasses.filter(_.flags.is(Sealed))`); for a case of a sealed trait also `tagKey[T]`
   (`Symbol.requiredClass("scala.deriving.Mirror")`, `Type.show[T]` only as the by-name
   `typeName` of the error message, `Up/MacrosCommon.scala:8-32`), `tagName[T]` (`:487-519`:
   `TypeTree.of[T].symbol`, `<:< TypeRepr.of[scala.reflect.Enum]`, `TermRef`/`TypeRef`/`AppliedType`
   unapply, `typeSymbol.fullName`), `shortTagName[T]` (`:522-538`: `baseClasses`, `children`,
   `flags.is(Case)`).
7. `macroW`: `writeLength[T]` (`:233-279`: `Select.unique(v.asTerm, name)`, `getDefaultParamsImpl0`,
   `extractSerializeDefaults`), `writeSnippets[R, T, Writer]` (`:287-372`: `appliedTo`,
   `(tpe0.asType, writerTpe0.asType) match { case ('[tpe], '[writerTpe]) => … }` — two
   `TypeMatch.unapply` calls, a quote with two spliced types, three holes and `summonInline`,
   `Expr.block`).

Sum path (`macroRAll`/`macroWAll`, `defineEnumVisitorsImpl` `:557-602`): `AppliedType(_, List(arg))`
over `Tuple.Map[m.MirroredElemTypes, Reader]`, `=:= TypeRepr.of[EmptyTuple]`,
`flags.is(Trait|Abstract|Enum)`, `Symbol.newVal(spliceOwner, name, tpe, Flags.Implicit | Flags.Lazy,
noSymbol)`, `TypeApply(Select(prefix.asTerm, prefix.asTerm.tpe.typeSymbol.methodMember("macroR").head),
List(TypeTree.of(…)))`, `ValDef(sym, Some(rhs))`, `Block(defs, Ident(sym.termRef))`, `asExprOf`; the
retype then expands `macroR[Circle]`, `macroR[Rect]`, `macroR[Shape]` inside the block (product
path each, `isMemberOfSealedHierarchy` true) — and their `summonInline[Reader[…]]`s must see the
block's `implicit lazy val`s, which `Yi/host.rs:38-40` refuses ("implicit search in an inline body
with local implicits") [V code, I that it fires here]. A `derives` clause always takes this path
(`Up/MacroImplicits.scala:49`), so the sum program needs it; `macroRW`'s own `SumOf` branch
(`summonAll` + `Reader.merge`, `Readers.scala:228-234`) avoids `newVal` but only for a hand-written
`given`.

### 3.3 mainargs (`Ma/Macros.scala`; entry `ParserForMethods.apply` → `parserForMethods`, `:8-46`)

In order: `TypeRepr.of[B].typeSymbol.memberMethods` (`Qi:3077`, all term members with `Method`,
inherited included), `Symbol.requiredClass("mainargs.main")` (`:6`, `Qi:2753`),
`getAnnotation(sym)` as a `Term` (`Qi:3002`), `Symbol.pos: Option[Position]` (`Qi:2978`) for
`sortBy`; `createMainData` (`:113-178`): `paramSymss`, `flags.is(HasDefault)`, `getDefaultParams`
(`:239-267`: `method.owner.tree.asInstanceOf[ClassDef].body`, `DefDef(name, _, _, _)`,
`deff.symbol`, `Select(owner.asTerm, deff.symbol)`), `param.tree.asInstanceOf[ValDef].tpt.tpe`,
`VarargTypeRepr` (`:94-105`: `AnnotatedType`/`AppliedType`/`TypeRef` unapply, `defn.RepeatedAnnot.typeRef`,
`defn.ScalaPackage.termRef`, `=:=`), `annotParam.getAnnotation(arg).map(_.asExprOf[mainargs.arg])`
(the typed `new arg(short = 'f', doc = "…")` spliced into the result), `readerTpe.asType match {
case '[t] => … }`, `'{ Some((b: B) => ${ applyAndCast(f, 'b) }) }` → `case '{ $v: `t` }` (a typed
hole with a spliced type — works today [V mechanism]), `Expr.summon[TokensReader[t]]` →
`Implicits.search` (`L/Expr.scala:309-315`, `Qi:2669-2701`), `Expr(param.name)`, a quote with a
spliced type and four holes, `Expr.ofList`; `call` (`:199-236`): `methodOwner.asTerm.tpe.memberType(method)`,
`methodType.memberType(param)`, `'{ $ref(${Expr(i)}) }`, `'{ $x.asInstanceOf[t] }.asTerm`,
`select(method).appliedToArgs(terms)` (`Qi:479`), `asExpr`/`asExprOf[T]`; `Typed`/`Inferred`/
`AppliedType.apply` for a `Leftover` parameter only. Error paths: `report.errorAndAbort(msg)` and
`(msg, pos)`, `.show` on `TypeRepr`s and `Expr`s, `Type.show[t]` in the "No TokensReader" message.
`parserForClass` (`:48-86`) adds `TypeRef(a, b)` unapply, `TermRef.apply`, `Ident(termRef)`,
`companionModule.tree.asInstanceOf[ValDef].tpt.tpe.asType`, `memberMethod("apply")`,
`companionModule.pos` — not in the program.

`Implicits.search` needs the typer's search from inside the running macro: `Yi/host.rs:20`
`Host::summon` is reachable from the `Inlining` only, while the typer is suspended in
`expand_macro` (`Y/inline_host.rs:85`) and the VM's native holds `M/host.rs`. Port: `MacroCall`'s
expansion lends a boxed `FnMut(Lent, TypeId, Span) -> Result<Option<TNodeId>>` (the typer's
`implicit_arg` in `call_ctx`, `Y/inline_host.rs:63-79`) to `VmMacros::expand` (`M/lib.rs:23`) and
`Host` keeps it beside the tables; the native runs in Rust on the VM's thread, so the typer is not
re-entered through Scala (ADR 0029 holds) [V structure, I sizes].

## 4. MRs

Sizes are Rust + overlay Scala lines of diff [I]; each MR adds a fixture macro in
`tests/macros/lib/Macros.scala` for its operations and keeps `just gates`; library programs go to
`tests/aot/<name>` + `crates/scivolo/tests/libraries.rs` as research 30 §4 says. `M/reflect.rs`
dispatch and `O/ReflectImpl.scala` are split by subject in E (research 26 §4); one coder on the
dispatch at a time.

| MR | scope | files | size | unlocks | after |
|---|---|---|---:|---|---|
| FP0 | §2.1–2.4 four typer fixes, shapes as goldens | `Y/extension_calls.rs`, `Y/implicits/search.rs`, `Yi/proxies.rs`, `Yu/selections.rs` | ~60 | fp, upk reach the macros | — |
| R1 | W6-lite `report.errorAndAbort` ×3; E+ `Term.tpe`, `widenTermRefByName`, `ConstantType`, `isExprOf`/`asExprOf`, `valueOrAbort`; W1-lite `=:=`, `<:<`, `widen` | `M/reflect/terms.rs`, `types.rs`, `O/ReflectTerms.scala`, `O/QuotesImpl.scala`, `Yi/reflect.rs` | ~110 + ~70 | `Expr.value`/`Exprs`; `LiteralStr`, `CharsWhileIn`, `~` | E |
| R2 | P1-lite literal quote patterns; W4-lite `Match`/`CaseDef`/`Wildcard`/`Block.apply`/`Select(qual, sym)`/`TypeApply.apply`/`appliedToArgs`/`Ident(termRef)`; retype of built matches; `tests/aot/fastparse` (`~`, `rep`, `CharIn`, `!`, `map`, `End`) | `Yu/quote_patterns.rs`, `Yi/quote_match.rs`, `M/reflect/terms.rs`, `Yi/retype.rs`, `O/` | ~170 + ~50 | `rep`, `CharIn`, `StringIn`; fastparse passes | R1, S1 (`P`), I1 (`CharIn`), FP0 |
| R3 | P3 `TypeMatch`/`'[t]` (research 26 §3.4); W1 `appliedTo`, `memberType`, `substituteTypes`, `dealias`, `isSingleton`, `baseClasses`, `classSymbol`, TypeTests + unapply `AppliedType`/`TypeRef`/`TermRef`/`AnnotatedType`, `AppliedType.apply`, `Symbol.typeRef`/`termRef`, `defn.RepeatedAnnot`/`RepeatedParamClass`/`ScalaPackage` | `Yu/quotes.rs`, `O/QuotesImpl.scala`, `M/reflect/types.rs`, `Yi/reflect.rs` | ~220 + ~80 | every `'[t]` and type extractor of upickle, mainargs | R1 |
| R4 | W2a `primaryConstructor`, `paramSymss`, `caseFields`, `companionClass`, `isType`, `methodMember(s)`, `memberMethods`, `children` (`sealed.rs`), `requiredClass`, `classSymbol`, `Symbol.pos` | `M/reflect/symbols.rs`, `M/positions.rs`, `O/ReflectSymbols.scala` | ~130 + ~50 | symbol walks of upickle, mainargs | S1 |
| R5 | W2b `annotations`/`getAnnotation` as terms (script symbols from `Yn/annotation_trees.rs`; library symbols unpickled on demand — `Universe` keeps classes only, `crates/tasty/src/symbols/mod.rs:78`); `Symbol.tree` for script definitions (`ClassDef.body`, `DefDef`/`ValDef` unapply, `Tree.symbol`, `ValDef.tpt`, `TypeTree.tpe`) over E's `kind`/`children` | `M/reflect/symbols.rs`, `trees.rs`, `Yi/reflect.rs`, `O/` | ~140 + ~70 | `@key`, `@arg`, defaults of both libraries | E, R4, A1 |
| R6 | W3 `Implicits.search`/`Expr.summon` (§3.3); `tests/aot/mainargs` (`@main`, `@arg`, a default, `runOrExit`) | `Yi/macros.rs`, `Y/inline_host.rs`, `M/lib.rs`, `M/host.rs`, `O/ReflectImpl.scala` | ~85 + ~30 | mainargs passes | R2, R3, R4, R5 |
| R7 | `Select.unique`/`overloaded`, `Expr.block` on definitions, `summonInline` in retyped quotes checked; `tests/aot/upickle` product (`P` read/write, a default) | `M/reflect/terms.rs`, `Yi/retype.rs`, `O/` | ~100 + ~40 | upickle product | R2–R5 |
| R8 | `Symbol.newVal` (`Yi/splice.rs:24` `fresh`), `ValDef.apply`, blocks of definitions, `summonInline` seeing a built block's implicit lazy vals (`Yi/host.rs:38`); upickle sum in the program | `M/reflect/symbols.rs`, `Yi/host.rs`, `Yi/compiletime.rs`, `O/` | ~150 [I] | upickle sum (`derives` on a sealed trait) | R7 |

Totals: fastparse FP0 + R1 + R2 ≈ 400 (+ S1, E, I1 in flight); mainargs + R3–R6 ≈ 800 more;
upickle + R7 ≈ 140 more, + R8 ≈ 150 for the sum path. mainargs' slices are upickle's, so MA costs
little on its own and UP's product path is nearly free after it.

## 5. Order, risks, conflicts

Order: FP0 now (typer only, parallel to S1/E) → R1 → R2 → R4 → R3 → R5 → R6 → R7 → R8. fastparse
first: its own two MRs are the smallest and end a library; mainargs second (R3–R6 are shared
slices, and `@main` scripts are the common case); upickle last, its sum path (R8) the one
uncertain piece.

- R2's retype of macro-built `Match` trees and R7/R8's of built definitions: `Yi/retype.rs` must
  accept trees the typer did not type (research 26 §4's W4 risk); the fixture macro guards it.
- `@tailrec` on a `def` inside `repMacro0`'s quote (`Fp/MacroRepImpls.scala:69`): the unpickler has
  no `ANNOTATION` case [V grep `Yu/`]; probe with the fixture before R2 (if it refuses, skip
  annotations on quoted definitions, ~10 lines).
- R8: local implicits of a built block visible to `summonInline` is new typer surface; if it
  resists, upickle's sum path stays scala-cli's while the product path passes.
- `Type.show` (S) stays unneeded for the three programs; `Symbol.toString` in upickle's error
  messages (`Up/macros.scala:160,259`) is the overlay's `SymbolImpl` — add `toString = fullName`.
- Conflicts: FP0 touches `Y/implicits/search.rs` beside research 28's search MRs and
  `Yu/selections.rs` beside nothing in flight; R1–R8 live in the files E splits (one coder on the
  dispatch); R6's `MacroCall` field is additive to `Yi/macros.rs`; X1 (os-lib) is independent.
