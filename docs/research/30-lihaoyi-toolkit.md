# 30 — com-lihaoyi toolkit, stage 1 (#144)

Date: 2026-10-21. Probes of the published `_native0.5_3` jars through `scivolo-aot` (own frontend,
main d669a7fc) and `scivolo --dep`; claims are [V] verified by running or reading, [I] inferred.
Paths: `T/` = `ref/scala3/compiler/src/dotty/tools/dotc/typer/`, `Qi/` =
`ref/scala3/compiler/src/scala/quoted/runtime/impl/`, `Y/` = `crates/typer/src/typer/`, `Yn/` =
`crates/typer/src/namer/`, `M/` = `crates/macros/src/`, `O/` =
`overlay/src/scala/quoted/runtime/impl/`, `A/` = `crates/aot/src/`. Probes live in
`tmp/eval-lihaoyi/<name>/Main.scala` (one directory each, `//> using dep` inside, run with
`just aot-src tmp/eval-lihaoyi/<name> Main`); dependency-free shapes of the failures in
`tmp/eval-lihaoyi/shapes/*.scala` (`just typer-dump`). Versions: os-lib 0.11.9-M8, ujson/upickle
4.4.3, fastparse 3.1.1, mainargs 0.7.8, pprint 0.9.6, sourcecode 0.4.4. Follows
[research 26](26-macros-next.md) (macro needs of these libraries, §2.3) and
[research 28](28-adaptation-members-implicits.md).

**Recommendation.** ujson and pprint work today in both backends (pprint but for the applied
package `pprint(x)`, §2.11); os-lib works in the interpreter
but for one typer gap (implicit parameters of function type, §2.2) that fastparse shares. Six
small typer fixes (§4: M1, P0, F1, O1, A1, I1, ~220 lines together) and one AOT fix (L1) clear every
non-macro refusal. The rest is macro API (#40): symbol flags for sourcecode (S1, small), term
extractors for os-lib's path literals (E), and three large slices for fastparse, upickle and
mainargs that are research 26's P1/P3/S/W1–W4/W6 — those three are the uncertain part of the
issue and go last, each measured by its library's test program.

## 1. Probes

| library | probe | result (first refusal) | next failures (worked around in a copy) | § |
|---|---|---|---|---|
| ujson | `ujson` read, `Obj`/`Arr`, `write(indent = 2)` | runs, correct (aot and `scivolo --dep`) [V] | — | — |
| pprint | `pprintln` of case classes, `width`, `.plainText`, `pprint.log` | runs, correct (aot and `scivolo --dep`) [V]; `log` exercises sourcecode `Line`/`FileName`/`Text`; the typical `pprint(x)` refuses "package used as a value" [V] | `TPrint` (type names) not probed: a macro of research 26 W1/S [I] | 2.11 |
| sourcecode | `sc`: `Line`, `FileName`, `File`, `Name`, `Enclosing`, `Text`; `sc2`: + `Pkg`, `Args` | `Line`/`File`/`FileName`/`Text` work; `Name`, `Enclosing`, `Pkg`: "no method …ReflectImpl::FlagsMethods" from `Util.isSynthetic` [V] | `Args`: "no method SymbolMethodsImpl::isDefDef", then `Symbol.tree` [V, I] | 2.6 |
| os-lib | `osl`: `/` with strings, `write.over`, `read`, `list`, `proc(...).call()`, `call(stdin = "piped")` | "adaptation" at the data argument of `os.write.over(p, "hi")` (`String` → `os.Source`) [V] | `osl3`: without `Source`/`ProcessInput` conversions the typer passes; `scivolo --dep` runs all of it (files and `os.proc`) correctly [V]; aot stops in codegen: "link-time property …GenericProcessWatcher$…isEnabled" [V]; `osl6` (files only) runs as an executable [V] | 2.2, 2.7 |
| os-lib | `osl5a–f`: `os.pwd / "tmp"` | types: the typer picks `PathChunk.StringPathChunkInternal`, a `private[os]` conversion dotty rejects (`osl5e` compiles for us, not for dotty); dotty picks the macro `stringPathChunkValidated` [V dump] | with the macro imported (`osl5c/d`): "no method …ReflectImpl::InlinedTypeTest" [V] | 2.3, 2.8 |
| upickle | `upk`: `case class P … derives ReadWriter`, `sealed trait Shape derives ReadWriter`, `write`/`read` | "Mirror of a type with type variables" at `write(P(…))` [V] | `upk-a/b/c`: the same for `summon[ReadWriter[P]]`, `macroRW`, and `summon[Writer[P]]` with a plain `implicit val` in `P`'s companion — the cause is the candidate `superTypeWriter` of `upickle.default`, not the derivation [V] | 2.1 |
| fastparse | `fp`: `def num[$: P] = P(CharIn("0-9").rep(1).!.map(_.toInt))`, `~`, `End`, `parse` | "adaptation" at `"0-9"`: an inline repeated parameter [V col, I cause] | `fp2` (`CharPred`): "ambiguous overload resolved by type arguments of a later clause" at `.rep(1)` [V]; `fp3` (`CharsWhileIn`): "adaptation" at `parse("123", …)` (`String` → `ParserInputSource`) [V]; then the macros `P`, `~`, `rep`, `LiteralStr`, `CharIn` (research 26) [I] | 2.2, 2.4, 2.5 |
| mainargs | `ma`: `@main def run(@arg(short = 'f', doc = …) foo: String, …)`, `ParserForMethods(this).runOrExit` | "annotation" at `@main` [V] | `ParserForMethods.apply` is a macro over `Symbol.annotations`, `paramSymss`, `Symbol.tree`, `Expr.summon`, `Type.show` [V sources] | 2.9 |

Backends: `scivolo --dep COORD file.scala` compiles through the own frontend with the
dependency's TASTy (#128 is done [V ujson, os-lib]); a `//> using dep` directive still sends a
script to scala-cli (`crates/scivolo/src/frontend.rs:64-65`, #118) [V]; `scivolo-aot` reads the
directive (`A/frontend.rs:32-44`) [V]. The REPL was not probed.

## 2. Causes

### 2.1 Mirror of a type variable (upickle) — `Y/implicits/product_mirrors.rs:27-29`

`inline given superTypeWriter[T: Mirror.ProductOf : ClassTag, V >: T : Writer : Mirror.SumOf]
(using NotGiven[CurrentlyDeriving[V]]): Writer[T]` (upickle `Writers.scala`, imported by
`import upickle.default.*`) is tried for `Writer[P]`; its clause `Mirror.SumOf[V]` is searched
with `V` an uninstantiated variable (`?V >: P`). `synthesized_mirror` refuses
("Mirror of a type with type variables"), a failure that is not a misfit, so the whole search
stops [V shape `mirror_var.scala`]. dotty's `makeMirror` (`T/Synthesizer.scala:654-664`) calls
`fullyDefinedType(mirroredType, …)` (`T/Inferencing.scala:44`, `ForceDegree.all`), which
instantiates `V := P`; `P` is no sum, the candidate fails, the search goes on to `P`'s own given
[V dotty]. **Fix (M1):** `fully_defined_as(mirrored, true)` (`Y/infer.rs:123`, already dotty's
`fullyDefinedType`) in place of the `is_fully_defined` check, inside the attempt's constraint
snapshot (`Y/implicits/attempts.rs:69-79` restores it). ~10 lines. Not in #129's list; it is
mirror work and should be noted there. M1 also needs wildcard arguments to compare like dotty's
`WildcardType` (`isSubArg`; `ref/scala3/compiler/src/dotty/tools/dotc/core/TypeComparer.scala:362,
:484`) in `Cmp::sub_arg` (`crates/tasty/src/subtype/applied.rs`), else the divergence check's
`wildTp =:= wildPt` fails and the search stops with "implicit search too deep" [V]; and
`wild_approx` (`Y/wild_approx.rs`) approximates bounds with variables recursively, as dotty's
`wildApproxBounds`.

### 2.2 Implicit parameters of function type (os-lib, fastparse) — `Y/implicits/contextual.rs:111-120`

`implicit class WritableSource[T](s: T)(implicit f: T => geny.Writable)` (os-lib `Source`),
`implicit def readableWritable[T](t: T)(implicit f: T => Readable)` (geny), `IterableShellable`,
`makeSourceInput` (`ProcessInput`), and fastparse's `fromParserInput[T](t: T)(implicit conv: T
=> ParserInput)` all take a *function* that an `implicit class StringWritable(s: String)` or
`implicit def fromString(s: String)` supplies. dotty's `candidateKind` (`T/Implicits.scala:224-235`):
for a `pt` that `isFunctionNType`, every eligible implicit is a `Candidate.Value`, and
`typedImplicit` (`:1184`) adapts the method reference to the function type, i.e. eta-expands it
[V]. Ours: `is_eligible` rejects any candidate whose type after the implicit clauses is a
`Method`, and `try_implicit` (`search.rs:149-153`) does the same, so no candidate fits and
`adapt` refuses "adaptation" [V shape `view_function.scala`, V probes osl, fp3]. Research 28 §3
saw the same shape in dotty's t3346g (MR P). **Fix (F1):** when `pt` is a `FunctionN` class type,
a candidate with one explicit clause of N parameters is eligible, and `try_implicit` adapts its
reference against `Pt::Type(pt)` through `adapt` (`Y/adapt.rs:50-62` eta-expands a method
against a function type) before the conformance check. ~40 lines.

### 2.3 Accessibility of library implicits (os-lib) — `Y/implicits/candidates.rs:122-124`

`candidate_ref` checks accessibility for script symbols only; a `private[os]` implicit of the
library (`PathChunk.StringPathChunkInternal`) is a candidate outside `os` and, being declared
in the object itself, wins the owner ranking (`Y/implicits/ranking.rs:108`, dotty's
`compareOwner` `T/Applications.scala:2106`) over the two inherited conversions [V dump osl5f].
dotty's `isAccessible` (`T/Implicits.scala:121,293,395`) drops it before ranking and the macro
conversion `stringPathChunkValidated` (`PathChunkMacros`, which derives from
`StringPathChunkConversion`) wins [V dotty]. Consequence: our `os.pwd / "tmp"` compiles and runs
(osl5f, osl6) without the literal validation, and `import os.PathChunk.StringPathChunkInternal`
(osl5e) compiles where dotty errors. **Fix (X1):** `is_accessible_from` for every candidate, an
inaccessible one a misfit. ~15 lines; after E (§2.8), since then the macro is chosen.

### 2.4 Overloads with type parameters after the receiver clause (fastparse) — `Y/mapped_alternatives.rs:44-50`

`p.rep(1)` with `extension [T](p: P[T]) inline def rep[V](using …)` and `extension [T](p: =>
P[T]) inline def rep[V](inline min: Int = 0, …)(using …)`: both alternatives have the type
`[T](p)[V](…)`; after skipping the receiver clause the rest is still a `Poly`, which
`skip_param_clause` refuses [V shape `rep_overload.scala`]. dotty's `skipParamClause`
(`T/Applications.scala:2717-2732`) flattens the outer and inner `PolyType`s into one
(`derivedLambdaType(resType = rt).flatten`) and goes on with `resolveMapped` (`:2811`) [V].
**Fix (O1):** merge the binders (`Type::Poly` with the inner parameters appended, references
rebased) instead of the refusal. ~30 lines.

### 2.5 Inline repeated parameters (fastparse) — `Y/adapt.rs:116-120`

`inline def CharIn(inline s: String*)`: the literal `"0-9"` is typed against the raw
`<repeated>[String]` formal and refuses "adaptation" [V col; I cause by research 28 §2's
inline-param-semantics row, `42 → <repeated>[Int]`]; `CharsWhileIn(inline s: String)` passes
(fp3). dotty types inline arguments against the formal's element type like any argument
(`typedArgs` for `RepeatedParamClass`) and the inliner binds the sequence. This is #82's I
(research 27 §2.2, research 28 MR I). **Fix (I1):** in the inliner's argument typing, a repeated
inline parameter takes a `SeqLiteral` of the arguments like a non-inline one; ~40 lines in
`Y/` and `crates/typer/src/inlining/`.

### 2.6 Symbol flags and predicates in `quotes.reflect` (sourcecode) — `O/ReflectImpl.scala:130-144`, `M/positions.rs:19-51`

`sourcecode.Macros.nameImpl`/`enclosingImpl`/`pkgImpl` walk `Symbol.spliceOwner.owner…` with
`Util.isSynthetic`: `s.flags.is(Flags.Synthetic)`, `isClassConstructor`, `isLocalDummy`,
`flags.is(Flags.Macro)`, `Flags.Inline`, `Flags.Scala2x`, `isPackageDef`, `isClassDef`,
`isValDef`, `isDefDef`, `moduleClass`, `defn.RootPackage`, `defn.RootClass` [V sources]. Our
overlay has `owner`, `name`, `fullName`, `spliceOwner`, `noSymbol`, no `Flags`, no `FlagsMethods`
(`O/ReflectImpl.scala:19-24`) and no `defn` [V]; the host answers `owner`/`name`/`fullName`
(`M/positions.rs:37-51`). dotty: `QuotesImpl.scala:2959` (`flags`), `:3014-3030` (predicates),
`:3167` (`moduleClass`), `:3217` (`defn`), `:3358` (`FlagsMethods`). `Args` additionally reads
`Symbol.tree` as a `DefDef`/`ClassDef` (`:2997`) — research 26 L leaves it out. This is research
26's L (second half: "symbol identity"). **Fix (S1):** a host op `flags` returning a bit set the
overlay maps onto `Flags` values (Synthetic, Macro, Inline, Scala2x, Module, Case, Sealed,
Abstract, Trait, Enum, HasDefault, Implicit, Lazy — the ones the six libraries test), ops
`kind` and `moduleClass`, the `isX` predicates over `kind`, `defn.RootPackage/RootClass`.
~80 Rust, ~80 Scala. Unblocks sourcecode `Name`/`FullName`/`Enclosing`/`Pkg`, fastparse's `P`
(takes a `sourcecode.Name`), and the `flags` reads of upickle, mainargs and pprint's `TPrint`.

### 2.7 AOT: link-time accessors computed from others (os.proc) — `A/codegen/linktime/eval.rs`

`GenericProcessWatcher.isEnabled` is `@resolvedAtLinktime def isEnabled = LinktimeInfo.
isMultithreadingEnabled` (`ref/scala-native/javalib/src/main/scala/java/lang/process/
GenericProcessWatcher.scala:13-14`) and `watchForTermination` tests it in a `linktime if` whose
property *name* is the accessor's mangled name [V]. The VM resolves such a name by calling the
accessor (`crates/vm/src/linktime.rs:128-141` `property_or_accessor`) [V]; the AOT's `decide`
knows the `LinktimeInfo` accessors by name only (before L1) and fails on any other [V].
**Fix (L1, done):** as Scala Native's `LinktimeValueResolver` (`interpretLinktimeDefn`),
`decide` evaluates the body of the accessor a mangled property name names: `resolveProperty`
reads, `linktime if`s and jumps to a `ret`, with the executable's property values
(multithreading `false`, so `isEnabled` is `false` and no watcher thread starts), once per
accessor, refusing cycles; calls of such accessors become the constant. This replaces the table
of `LinktimeInfo` accessors; test `tests/programs/linktime` (user accessors computed from
others) [V]. After it, osl3 compiles as an executable and runs up to `os.proc(…).call()`,
which stops in `Thread.startInternal` ("no definition of …UnsupportedFeature") [V]. The reason:
`os.proc(…).call()` starts pumper threads for stdout/stderr and a shutdown hook thread (os-lib `ProcessOps.scala` `spawn`) [V source] — executables are single-threaded (ADR
0028), so `call()` in an executable is open until #115 grows threads; the `os.proc` part of the
test program runs through `scivolo --dep` (green threads), where it passes today.

### 2.8 Term extractors in macros (os-lib path literals, fastparse) — research 26 E

`stringPathChunkValidatedImpl` matches `s.asTerm` against `Inlined(_, _,
Literal(StringConstant(literal)))` and lifts `Expr(segments: Seq[String])` (os-lib
`os/src-3/Macros.scala`); the overlay has `Literal.unapply` only (`O/ReflectImpl.scala:38`), so
the first `InlinedTypeTest` is a missing method [V osl5c]. fastparse's `literalStrMacro`,
`charInMacro` (`Varargs`, `Expr.value`) and `parsedSequence0`/`repMacro0` read terms the same way
[V research 26 §2.3]. dotty: `Qi/QuotesImpl.scala:1090-1115` (`InlinedTypeTest`, `Inlined`,
`InlinedMethods`), `:618` (`Literal`). **Fix (E):** as research 26 §3.3: host ops `kind` and
`children` over `tpd::Tree`, the `TypeTest`s and `unapply`s of `Inlined`, `Literal`, `Typed`,
`Block`, `Apply`, `TypeApply`, `Select`, `Ident`, `Repeated` in the overlay; `Varargs`,
`Expr.ofSeq`/`ofList` and `ToExpr[Seq[T]]` through `Build`. ~150 Rust, ~180 Scala.

### 2.9 Annotations of library classes (mainargs) — `Yn/annotations.rs:19-38,53-61`

`@main` and `@arg(short = 'f', doc = "…")` are `class main(val name: String = null, val doc:
String = null) extends ClassfileAnnotation` and `class arg(name, short: Char = 0, doc,
noDefaultName, positional, hidden)` (mainargs `Annotations.scala`) [V source]. The namer accepts
18 annotation classes by name and refuses every other ("annotation") [V]. dotty's
`addAnnotations` (`T/Namer.scala:943`), `typedAheadAnnotationClass` (`:1882`) and
`typedAnnotation` (`T/Typer.scala:3049`) type any annotation as a `new C(args)` with named and
default arguments and keep it on the symbol; phases react to the classes they know. **Fix (A1):**
replace the allowlist by (1) typing every annotation whose class is a `StaticAnnotation`
subclass as an application (named and default arguments included) and keeping the typed tree
on the symbol — the source of `Symbol.annotations` for macros later (research 26 W2) — and (2)
a *denylist* of annotations with code-generation semantics the lowering does not implement
(`scala.scalanative.annotation.*` and `unsafe.*`: `extern`, `exported`, `stub`, `link`,
`define`, `name`, `resolvedAtLinktime`, `alwaysinline`, `nooptimize`, `nospecialize`,
`safePublish`; `scala.native`), refused as today. ~60 lines. After it, mainargs is a macro:
`parserForMethods` (`mainargs/src-3/Macros.scala`) uses `Symbol.requiredClass`,
`memberMethods`, `getAnnotation`/`annotations`, `paramSymss`, `primaryConstructor`,
`companionModule`, `Symbol.tree` as `DefDef`/`ValDef`/`ClassDef`, `Expr.summon`, `Expr.ofList`,
`TypeRepr.show`/`Type.show`, `TypeRepr.widen`/`memberType`/`=:=`/`asType`, the extractors
`TypeRef`/`TermRef`/`AppliedType`/`AnnotatedType`/`Typed`/`Inferred`/`Select`/`Ident`,
`Select`/`appliedToArgs`/`asExprOf`, `defn.RepeatedAnnot`, `report.errorAndAbort` [V source] —
research 26's W2, W3, S, E, W4.

### 2.10 upickle after M1 — research 26 P3, S, W1, W2, W4, W6

`derives ReadWriter` becomes `given derived$ReadWriter: ReadWriter[P] = ReadWriter.derived`
(`Y/deriving.rs:115-116`); `derived` is a member of `implicit class ReadWriterExtension(r:
ReadWriter.type)` (upickle `MacroImplicits.scala`), so the selection needs the member-view path
(`Y/implicits/conversions.rs:47` with `Goal::Member`) [I, risk]. `derived` → `macroRWAll` →
`inline m match { case m: Mirror.ProductOf[T] => … }` (the Circe shape, works) → the 22 macro
impls of `upickle/implicits/src-3/upickle/implicits/macros.scala`; the product path alone
(`validateFlattenAnnotation`, `tagKey`, `fieldLabelsImpl0`, `getDefaultParamsImpl0`,
`writeSnippets`, `writeLength`, `keyToIndex`, `allReaders`, `applyConstructor`,
`storeDefaults`, `isSingleton`, `isMemberOfSealedHierarchy`) uses `'[t]` patterns on `asType`
(P3), `summonInline` inside quotes, `Expr.ofList`/`ofTuple`/`block`, `TypeRepr.typeSymbol`/
`typeArgs`/`appliedTo`/`widen`/`memberType`/`substituteTypes`/`baseClasses`/`isSingleton`/
`=:=`/`<:<`/`dealias` (W1), `Symbol.caseFields`/`primaryConstructor`/`paramSymss`/
`companionClass`/`companionModule`/`annotations`/`flags`/`tree`/`children`/`methodMember`/
`newVal`/`termRef`/`isType` (W2), `Match`/`CaseDef`/`Wildcard`/`Literal`/`Select.unique`/
`Select.overloaded`/`Typed`/`TypeTree.of`/`Ref`/`ValDef`/`Block`/`Ident`/`TypeApply`/
`appliedToTypes` (W4), `report.error(msg, pos)` (W6), `Type.show` on error paths (S) [V source
survey]. The sum path adds `Symbol.children`, `defineEnumVisitors` (`Symbol.newVal` + `ValDef` +
`Block`). This is the largest consumer of the reflect API among the six libraries.

### 2.11 Applied package objects (pprint) — `Y/access.rs:41-43`

`pprint(x)` applies the package `pprint`, whose package object extends `PPrinter`. `check_value`
refuses a package as a value even when it is applied [V]. dotty's `checkValue`
(`T/Typer.scala:773-779`) skips the check for a `FunOrPolyProto` or `SelectionProto`, and
`tryInsertApplyOrImplicit` (`:4218-4243`) inserts `.apply` [V]. **Fix (P0):** no package check
when `applied`, and the apply insertion resolves `apply` in the package object. ~20 lines [I].
**Done:** no value check when applied to arguments or type arguments; the existing apply
insertion finds `apply` among the package object's members (golden
`typed/package_object_apply.scala`) [V].

## 3. Per library: what remains

| library | typer / lowering | aot runtime | macros |
|---|---|---|---|
| ujson | — | — | — |
| pprint | — (P0 done) | — | `TPrint` only (not in the test program) |
| sourcecode | — | — | S1 (flags, predicates, `defn`); `Args` also `Symbol.tree` (left out) |
| os-lib | F1; X1 (correctness, then E needed) | L1; threads for `os.proc` (open, ADR 0028) | E (path literals; only once X1 lands) |
| fastparse | I1, O1, F1 | — [I] | S1 (`P`), E, literal patterns `case '{-1}` (P1 subset), `Match` construction (W4 subset), `=:=` (W1 subset) |
| upickle | M1; member view on `ReadWriter.type` [I] | — [I] | P3, S, W1, W2, W4, W6 (§2.10) |
| mainargs | A1 | — [I] | S1, E, W2, W3, S, W4 (§2.9) |

## 4. Plan

Each row one MR, stacked in the listed order where a dependency exists; sizes are Rust (+ overlay
Scala) lines of diff [I]. Every typer MR adds its shape probe as a golden test (`typer-compare`
against dotty's oracle) and passes `just gates`, `dotty-lower-report`, `snapshot-diff programs
lower typer`. Library test programs go to `tests/aot/<name>/<name>.scala` + `.check`, run by
`check_expanded` in `crates/aot/tests/programs.rs:65-75` (skipped when the jars are not in the
Coursier cache, `crates/aot/tests/programs.rs:104-115`), and through `scivolo --dep` in
`crates/scivolo/tests/libraries.rs`.

| step | scope | files | size | test | after |
|---|---|---|---|---|---|
| T0 tests | `tests/aot/ujson`, `tests/aot/pprint` (`PPrinter.BlackWhite`), their `#[test]`s; `crates/scivolo/tests/libraries.rs` running the same (and circe) through `scivolo --dep` | `crates/aot/tests/programs.rs`, `crates/scivolo/tests/` | ~70 | the two programs | — |
| M1 mirror of a variable | §2.1 `fully_defined_as` in `synthesized_mirror`; wildcard arguments like dotty's `WildcardType`; recursive `wild_approx` | `Y/implicits/product_mirrors.rs`, `crates/tasty/src/subtype/applied.rs`, `Y/wild_approx.rs` | ~30 | `shapes/mirror_var.scala`; upk-c | — |
| P0 package apply | §2.11 an applied package object gets `.apply` | `Y/access.rs`, apply insertion | ~20 | `pprint(x)` in `tests/aot/pprint` | — |
| F1 function-typed implicits | §2.2 method candidates for `FunctionN` goals, eta-expanded | `Y/implicits/contextual.rs`, `search.rs` | ~40 | `shapes/view_function.scala`, dotty t3346g; os-lib `write.over`, fastparse `parse` | — |
| O1 poly after the receiver clause | §2.4 flattened `Poly` in `skip_param_clause` | `Y/mapped_alternatives.rs` | ~30 | `shapes/rep_overload.scala`; fp2 | — |
| A1 annotations | §2.9 any `StaticAnnotation` typed and kept, denylist for native code-generation annotations | `Yn/annotations.rs`, `Y/` (typed annotation trees on symbols) | ~60 | a golden with a user annotation class with defaults and named arguments; ma's `@main`/`@arg` type | — |
| I1 inline repeated parameters | §2.5 | `crates/typer/src/inlining/`, `Y/adapt.rs` | ~40 | dotty inline-param-semantics; fp `CharIn` | — (#82, one coder with the inliner) |
| L1 aot link-time accessors | §2.7 accessor bodies evaluated in `decide` | `A/codegen/linktime/`, `blocks.rs` | +260/−150 | `tests/programs/linktime` | — (#115) |
| T1 os-lib program | `tests/aot/oslib` (files, `os.walk`, `os.copy`) + `os.proc` through `scivolo --dep` in `libraries.rs` | tests | ~40 | osl | F1, L1 |
| S1 symbol flags | §2.6 `flags`, `kind`, `moduleClass` ops; `Flags`, `FlagsMethods`, `isX`, `defn` in the overlay | `M/positions.rs` (or a new `M/symbols.rs`), `O/ReflectSymbols.scala`, `O/ReflectImpl.scala`, `overlay/nir` rebuilt | ~80 + ~80 | `tests/aot/sourcecode` (`Line`, `File`, `Name`, `Enclosing`, `Pkg`, `Text`); fixture macro in `tests/macros/lib` | — |
| E term extractors | §2.8, research 26 §3.3 | `M/reflect.rs` split by subject, `O/ReflectImpl.scala`, `crates/typer/src/tpd/build.rs` | ~150 + ~180 | fixture macros matching `Inlined`/`Literal`/`Typed`; os-lib `stringPathChunkValidated` via import (osl5c) | S1 (shared files) |
| X1 library implicit access | §2.3 accessibility of every candidate | `Y/implicits/candidates.rs` | ~15 | osl5e refused as dotty; osl5f picks the macro | E |
| FP fastparse macros (large, uncertain) | `Expr.value` via E, `case '{-1}` literal patterns (closed patterns of literals only), `Match`/`CaseDef` construction, `=:=`; `tests/aot/fastparse` | `crates/typer/src/inlining/quote_match.rs`, `unpickle/quote_patterns.rs`, `M/`, `O/` | ~250 + ~100 | fp (grammar with `~`, `rep`, `!`, `map`, `End`) | S1, E, F1, O1, I1 |
| UP upickle macros (large, uncertain; 2–3 MRs) | P3 `'[t]`; W1 types; W2 symbols; W4 construction; W6 `report.error`; `Expr.block/ofList/ofTuple`, `summonInline` in quotes; `tests/aot/upickle` | as research 26 §3.4, §3.7 | ~700 + ~300 | upk (`derives ReadWriter` product and sum, `read`/`write`) | M1, S1, E |
| MA mainargs macros (large, uncertain) | W3 `Expr.summon`; S `Type.show`/`TypeRepr.show`; `Symbol.tree` as `DefDef`/`ValDef`/`ClassDef`; `memberMethods`, `getAnnotation`; `Typed`/`Inferred`/`AnnotatedType` extractors; `appliedToArgs`; `tests/aot/mainargs` | `M/`, `O/`, `crates/tasty/src/types/show_source.rs` (S) | ~450 + ~150 | ma (`@main` with `@arg`, `runOrExit`) | A1, S1, E, UP's W2/W4 |

Order: T0 → M1 → P0 → F1 → O1 → A1 → I1 → L1 → T1 → S1 → E → X1 → FP → UP → MA. The first nine
are independent of each other (I1 shares the inliner with #82's MRs, O1 `mapped_alternatives.rs`
with #83's overload work — rebase, no design conflict); together ~350 lines, after which ujson,
pprint and os-lib (interpreter; executable without `os.proc`) pass and sourcecode needs S1 only.
The macro steps are #40's and follow research 26's split (`M/reflect.rs` and `O/ReflectImpl.scala`
by subject before E; one coder on the dispatch at a time).

## 5. Overlaps and risks

- #128 (`--dep` for scripts) is done; #118 (`//> using dep` through the own frontend) still routes
  directive scripts to scala-cli — the test programs use `--dep` (interpreter) and the aot's
  directive reading.
- #129 mirrors: M1 is a fourth leftover (mirrors under open type variables); add it there.
- #83 / research 28: F1 is MR P's t3346g; X1 touches `candidates.rs` beside P's `imports from
  stable values`.
- #82: I1; #115: L1 and threads in executables (ADR 0028) — `os.proc` stays interpreter-only.
- #40 / research 26: S1 = L's second half (positions landed in a3ad61e0, `M/positions.rs`), E, P3, S,
  W1–W4, W6; P1 is needed only as literal patterns (fastparse `case '{-1}`), not the full
  matcher.
- Risks: A1 widens what the typer accepts — annotations with lowering semantics must stay on the
  denylist, checked against nscplugin's `PrepNativeInterop` list; X1 changes which conversion
  os-lib's `/` picks (macro instead of the package-private class), so it must follow E or os-lib
  regresses from "runs" to "macro fails"; UP's `derived` through an implicit class on
  `ReadWriter.type` is unprobed (§2.10) and may need `Y/deriving.rs` to type the rhs with the
  member-view path; FP's `rep` macro (`MacroRepImpls`) and `~` (`parsedSequence0`) were surveyed
  from research 26 only — probe them with the fixture before sizing the MR; the three large
  steps are each several hundred lines of reflect API, measured only by their library's
  program until `tests/run-macros` is wired (research 26 T).
