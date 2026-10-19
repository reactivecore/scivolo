# 26 — Next macro steps (#40, phase 4)

Date: 2026-10-19. Claims are [V] verified by reading the code or running it, [I] inferred. Paths:
`Qi/` = `ref/scala3/compiler/src/scala/quoted/runtime/impl/`, `Qd/` = `.../dotc/quoted/`, `X/` =
`.../dotc/transform/`, `N/` = `.../dotc/inlines/`, `Yi/` = `crates/typer/src/inlining/`, `Yu/` =
`crates/typer/src/unpickle/`, `M/` = `crates/macros/src/`, `V/` = `crates/vm/src/natives/`, `O/` =
`overlay/src/scala/quoted/runtime/impl/`, `R/` = `ref/scala3/tests/run/`. Follows ADR 0029 and
[research 25](25-circe-codec-derivation.md) §2.4 (what Circe needed, M4a/M4b landed 2026-10-19).
The refusal table `tmp/dotty/3.9.0-0.5.12/lower-results.tsv` read here is from the run *before*
today's integration MR (855 passed; ROADMAP: 990) [V], so its macro rows are an upper bound.

**Recommendation.** Fourteen MRs (§4), ~2,500 Rust and ~700 overlay Scala lines, most of them a
day or two each, ordered by what they unblock: T tooling first (two-file tests through the lowering report, `tests/run-macros` checked
out), since `tests/run` holds four macro tests and none is counted today (§2.1); then L positions
and symbol identity (sourcecode, munit `Location`, scalatest `Position.here` — every assertion in a
script, §2.3), E term extractors and `Varargs` (`FromExpr`/`Expr.value`, fastparse, os-lib), P3
`'[t]` type patterns (upickle, mainargs, tapir), S `Type.show` (every derivation's error path), B
`Term.betaReduce`, then the structural quote matcher P1/P2 (the ROADMAP's named gap, demanded by
`run-macros`, not by the surveyed libraries) and the reflect slices W1–W6 as libraries need them.

## 1. Where macros stand

Implemented [V, sizes `wc -l`]: the VM bridge (`V/quotes.rs` 179, `V/reflect.rs` 65; trait
`QuoteHost` `V/quotes.rs:18-37`), the host (`M/host.rs` 189, `M/reflect.rs` 117, `M/tasty_string.rs`
37, `M/lib.rs` 61), the typer side (`Yi/splice.rs` 178, `Yi/macros.rs` 50, `Yi/quoted.rs` 62,
`Yi/host.rs` 69, `Yi/quote_match.rs` 124, `Yi/reflect.rs` 147; `Yu/quotes.rs` 147,
`Yu/quote_patterns.rs` 103, `Yu/splices.rs` 58) — about 1,600 Rust lines — and the overlay
(`O/QuotesImpl.scala` 89, `O/ReflectImpl.scala` 200, `O/QuoteHost.scala` 99). The own corpus is
`tests/macros/lib/Macros.scala` (17 macros: quotes with term and type holes, typed-hole patterns
with a type binding `'{ $l: List[e] }`, `underlying`/`Literal`, `companionModule`/`declaredMethod`/
`Ref`/`select`, `typeArgs`, `Expr.ofTupleFromSeq`) and Circe (`tests/repl/deps/circe-derives.check`,
`crates/scivolo/tests/circe.rs`) [V].

Refused by name [V]:

| refusal | site |
|---|---|
| `Expr.show`, `Expr.matches`, `Expr.valueOrAbort`, `Expr.isExprOf`, `Expr.asExprOf` | `O/QuotesImpl.scala:14-21` |
| `TypeMatch.unapply` ("a quoted type pattern") | `O/QuotesImpl.scala:77-80` |
| `Ref.term`, `Literal.copy`, `TypeRepr.typeConstructorOf`, `Constant.show` | `O/ReflectImpl.scala:25,35,41,139` |
| any other `reflect` member: the `…MethodsImpl` classes are allocated without a constructor, a missing method is a VM error naming it | `O/ReflectImpl.scala:8-11`, ADR 0029 |
| a quote pattern other than type bindings + one `patternHole[T]` | `Yu/quote_patterns.rs:16,28-31` |
| a quote hole with a type argument | `Yu/quotes.rs:137` |
| a splice other than a static call with quote, `Type.of` or `Quotes` arguments | `Yi/splice.rs:62,85` |
| a `reflect` operation the host does not know (14 known) | `M/reflect.rs:19-68` |
| `scala.compiletime` operations outside nine (`error`, `codeOf`, `requireConst`, `constValue[Opt|Tuple]`, `summonInline`, `summonAll`, `summonFrom`) | `crates/typer/src/typer/inline_refs.rs:10-20,38-44` |

The `Expr.value`/`FromExpr` path is library code (`FromExpr.IntFromExpr` matches `asTerm` against
`Literal(IntConstant(x))`, `Inlined`, `Typed`, `Block` extractors) [I from research 25 §2.2's
`ToExpr` finding], so it fails on the first missing extractor, not on a named refusal.

## 2. What the corpus and the libraries need

### 2.1 dotty `tests/run` [V, grep over `R/` and the table]

`tests/run` holds **four** macro tests, all two-file and all skipped as "several files": 25333 (a
`'{ new Callback { class Worker … } }` quote with an inner class, no reflect), i22649
(`Printer.TypeReprStructure.show(TypeRepr.of[T])`, `Expr(str)`), quotes-add-erased (an
experimental `MacroAnnotation`: `Symbol.newMethod`, `ClassDef.copy`, `MethodType`, `report.error`)
and quotes-reflection (experimental: `typeSymbol.declarations`, `Symbol.tree`, `DefDef`/`ValDef`/
`TypeDef` extractors, `TermParamClause`). No `tests/run` test uses a quote pattern, a higher-order
hole, `'[t]`, `Expr.summon`, `Varargs`, `FromExpr` or `Position` (`quote-MacroOverride` and
`quoted-sematics-1` are not macro tests). No "typer refused" reason names quotes or splices; the
inline-related rows are #82's (17 "deferred, given or overriding inline definition", 9 "non-public
member in an inline method" — landed in !312 after this table was written, so stale) and the 12
"scala.compiletime operation" rows of §3.8.

`scala.compiletime` in `tests/run` (tests mentioning the identifier): `erasedValue` 29,
`summonInline` 21, `constValue` 15, `summonFrom` 11, `summonAll` 7, `testing.typeChecks` 6,
`ops.int`/`ops.boolean` 6, `error` 5, `constValueTuple` 4, `testing.typeCheckErrors` 4, `codeOf` 3
(i13181, i13183 refused "codeOf an argument other than an Int, Long or Boolean literal",
`Yi/compiletime.rs:158`), `constValueOpt` 1; of these only i12170, implicitMatch and summonFrom
passed in the table, the rest fall to #82, #43 or match types, not to macros.

So for `tests/run` the macro steps unlock at most 4 tests, two of them experimental API; the
counts that order §4 come from `tests/run-macros` (§2.2) and the libraries (§2.3).

### 2.2 dotty `tests/run-macros`

Not in the `ref/scala3` checkout: a blobless partial clone of tag 3.9.0 in cone mode with
`/tests/run/` and `/compiler/src/` (`ref/scala3/.git/info/sparse-checkout`, `.git/config`;
`just dotty-sources` adds `compiler/src`, `Justfile:229-230`) [V]; `git sparse-checkout add
tests/run-macros` fetches the blobs from GitHub (network) [I from the config]. About 450 two-file
tests [I], the corpus where structural patterns, higher-order holes, `FromExpr`, `Varargs` and the
printers are the common case; dotty's own `.check` files are the oracle for `Expr.show`/`Type.show`
text. Until it is checked out and T (§3.1) runs it, no step of §3 has a measured test count.

### 2.3 Libraries [V from the `-sources.jar`s in the Coursier cache unless marked]

| library (version) | macros | Quotes features | blocked by (§3) |
|---|---|---|---|
| Circe 0.14.10 `Configured{Decoder,Encoder,Codec}.derivedImpl`, `Default.mkDefaultImpl`, `getDefaultsImpl`, `typeNameImpl` (the only `scala.quoted` users of core and generic) | 6 | §research 25; the sum branch is the same pattern with `Mirror.SumOf[A]`, `$m.ordinal`, `ofSum`; `ConfiguredEnum*` and `semiauto`/`auto` are inline only (`summonInline` in an `inline match`, `codeOf`, `error`) | nothing on the success path (the sum path to probe: product pattern failing cleanly, `Mirror.Singleton` cases, macro recursion into `derived[case]`); `Type.show` on the error path only (S) |
| sourcecode 0.4.4 (`Macros.scala:9-64`: `Name`, `FullName`, `File`, `FileName`, `Line`, `Enclosing`, `Pkg`, `Text`, `Args`) | 12 | `Position.ofMacroExpansion`, `sourceFile.path/content`, `startLine`; `Symbol.spliceOwner/owner/name/fullName/flags.is(Synthetic|Macro|Inline)`, `isClassConstructor/isLocalDummy/isValDef/isDefDef/isClassDef/isPackageDef`, `moduleClass`, `noSymbol`, `defn.RootPackage`; `Text`: `asTerm.pos.sourceCode`; `Args`: `Symbol.tree` | L (all but `Args`) |
| munit 1.3.6 (`MacroCompat.scala:16-54`: `Location`, `Clue`, `compileErrors`) | 3 | `Position.ofMacroExpansion`, `sourceFile.getJPath/path`, `startLine`; `Clue`: `asTerm.pos.sourceCode`, `TypeTree.of[T].show(using Printer.TreeShortCode)`; `compileErrors`: `typeCheckErrors` | L, S (`Clue`), C |
| scalatest/scalactic 3.2.19 (`Position.here` `Position.scala:59-85`, `assert`/`assume` `BooleanMacro.scala`) | 5+ | `Position.here`: as munit's `Location`; `assert`: `Expr.show`, `underlyingArgument`, `Apply`/`Select`/`TypeApply`/`Block`/`DefDef`/`ValDef`/`Ident` extractors, `ValDef.let`, `appliedTo`, `widen` | L for every assertion's position; E + W4 + W5 for `assert`'s message |
| upickle 4.4.3 (`upickle/implicits/macros.scala:53-604`, 17 macros) | 17 | `'[t]` on `.asType`, `summonInline` in quotes, `Expr.ofList/ofTuple/block`, `Type.show`, `report.error/errorAndAbort`, `annotations`, `caseFields`, `primaryConstructor.paramSymss`, `companionClass`, `flags`, `memberType`, `baseClasses`, `=:=`, `appliedTo`, `methodMember`, `Match`/`CaseDef` construction, `Symbol.newVal` + `ValDef` + `Block` | P3, S, W1, W2, W4, W6 |
| fastparse 3.1.1 (`MacroInlineImpls`, `MacroRepImpls`: `"lit"`, `~`, `rep`, `CharIn`, `StringIn`) | 8 | `Expr.value/valueOrAbort` (String, Boolean, Int), `Varargs`, `Inlined`/`Typed` extractors, `case '{-1}` literal patterns, `Match` construction, `=:=`, `asExprOf` | E, P1 (literal patterns), W1, W4 |
| os-lib 0.11.8 (`os/Macros.scala`: path literals) | 4 | `Inlined(_, _, Literal(StringConstant(s)))`, `Expr(Seq[String])`, `errorAndAbort(… + s.show)` | E; W5 on the error path |
| pprint 0.9.6 (`TPrintImpl.scala`) | 1 | `TypeRef`/`AppliedType`/`RefinedType`/`TypeBounds`/`AnnotatedType` extractors, `=:=`, `typeSymbol.fullName`, `Type.show` | W1 (type extractors), S |
| mainargs 0.7.8 (`Macros.scala`) | 2 | `Expr.summon`, `Expr.ofList`, `Type.show`/`TypeRepr.show`/`Expr.show`, `'[t]`, `` '{ $v: `t` } ``, `paramSymss`, `Symbol.tree`, `Select`, `errorAndAbort(msg, pos)` | W3, S, P3, W2, W4 |
| tapir-core 1.13.32 (`EndpointInput.derived`, `Schema.modify`, `Schema.derived` via magnolia 1.3.23) | 15 files | `annotations`, `Expr.summon`, `Type.show`, `'[body[bt, cf]]` with bindings, `New`/`Select.unique`, `Inlined(_, _, Block(DefDef…))` path matching; magnolia: `flags`, `baseClasses`, `caseFields`, `allOverriddenSymbols`, `dealias`, `<:<`, `baseType`, `Expr.ofList/ofTuple` | W3, S, P3, W1, W2, W4, E |
| zio-json, jsoniter-scala, utest, kyo [I, no local sources] | — | zio-json = magnolia; jsoniter builds methods (`Symbol.newMethod`, `Lambda`, `Match`, `Implicits.search`) — the hardest; utest `assert` walks trees and `Expr.show`s; kyo `Position.ofMacroExpansion` | W4, W3, W5; L |

Three readings [I]: positions (L) are the cheapest broad win — every munit and scalatest assertion
in a script goes through `Location`/`Position.here`; term extractors and `FromExpr` (E) come before
structural quote *patterns* (P1), since libraries match terms with extractors and `'[t]` (P3), and
none of the surveyed ones uses a higher-order hole (P2 is a `run-macros` item); `Type.show` (S)
appears in nearly every derivation's error path and in upickle/mainargs/tapir's success path.

## 3. The steps

### 3.1 T — two-file tests and `run-macros` (tooling, ~80 lines)

`scripts/dotty-tests.py:59-76` compiles a test's source groups in order into one directory, each
group seeing the previous output (dotty's runner), so `Macro_1.scala`'s TASTy and NIR exist under
`tmp/dotty/…/nir/<test>`; `scripts/dotty-lower.py:44-46` then skips every test with more than one
source ("several files", 59 tests, 4 of them macros) and `crates/lower/src/bin/lower.rs:11-16`
takes one file and no classpath [V]. Port: `compile_one` writes each group to its own subdirectory;
`lower.rs` gains `--cp DIR…` read as scivolo reads `--cp` jars (TASTy into the universe, the
directory on the VM's classpath for the macro's NIR, `crates/scivolo/src/frontend.rs:52-`); the
report lowers the last group with the earlier ones on `--cp` and runs it with `--cp` of all. ~30
Python + ~50 Rust [I]. Plus the `run-macros` checkout recipe and a `lower-report` filter for it.
Unlocks: the 59 two-file tests of `tests/run` become measurable (55 of them non-macro: separate
compilation of plain classes, probably mostly passing [I]); of the 4 macro ones 25333 needs only a
quote with a class in it, i22649 `Printer.TypeReprStructure` (`Qi/printers/Extractors.scala`, W5),
the two experimental ones stay out.

### 3.2 B — `Term.betaReduce`: higher-order holes in quotes (~80 Rust, ~15 Scala)

A quote `'{ (x: Int) => $f(x) }` pickles the hole's content into the `termHole` closure
(`X/PickleQuotes.scala:105-151`: the `Hole` keeps `args` = the bound references, its content moves
out) [V]; the closure applies the function `Expr` and calls `Expr.betaReduce` →
`Term.betaReduce` (`Qi/QuotesImpl.scala:432-447`: a `TreeMap` running `X/BetaReduce.scala:75-184`
on each `Apply`) [V]. Our hole arguments already reach the closure as `Expr`s
(`Yu/quotes.rs:130-146`, `O/QuotesImpl.scala:40-45`) [V]; what is missing is `Term.betaReduce`.
`Yi/beta.rs` (217 lines) is dotty's `BetaReduce` for `Apply(Select(closure, apply), args)` with
purity-driven bindings [V]; the port is a `betaReduce` reflect op that walks the term and calls
`Inlining::beta_reduce` — `Reflect` (`Yi/reflect.rs:12-16`) holds `u`, `tpd`, `names` only, so it
gains the `fresh` symbols the host is lent (`Yi/splice.rs:22`) or the host builds an `Inlining`
for the op [I]; overlay: `object Term extends TermModule { def betaReduce }` and `Expr.betaReduce`
is library code over it. Unlocks: every macro with `$f(x)` in a *quote* (the `Lambda`-free form of
HO holes) and `Expr.betaReduce` callers — `run-macros` tests only; no surveyed library (§2.3).

### 3.3 E — term extractors, `Varargs`, `Expr.value` (~150 Rust, ~180 Scala)

`FromExpr`, `Expr.value`/`valueOrAbort`, `Expr.ofList`/`ofSeq`, `Varargs.unapply` and most
hand-written macros read terms through `Literal.unapply`, `Inlined.unapply`, `Typed.unapply`,
`Block.unapply`, `Apply`/`TypeApply`/`Select`/`Ident.unapply`, `Repeated.unapply` and the
`XTypeTest`s (`Qi/QuotesImpl.scala:97-1845`: per node a `TypeTest`, an `object X` with
`apply`/`copy`/`unapply` and `given XMethods`, 396 defs) [V]. Ours has `Literal.unapply` only
(`O/ReflectImpl.scala:36`) and `tpd::Tree` carries every node kind. Port: two generic ops, `kind`
(the node's tag, after `Inlined(_, Nil, _)` is *not* stripped — dotty's `Inlined` is a node macros
see) and `children` (the child numbers in dotty's order), over which the overlay writes the
`TypeTest`s and `unapply`s (~15 Scala lines a node, ten nodes); `Varargs.apply`/`Repeated.apply`
and `Typed.apply` as `Build` calls (`crates/typer/src/tpd/build.rs`) for `Expr.ofList`; `Expr.value`
is then library code. Unlocks: fastparse's operators (with P1's literal patterns and W4's `Match`),
os-lib's path literals, scalactic's `anyvals`, `Expr.ofList` in upickle, magnolia and mainargs,
and the `Inlined`/`Literal` idiom of most hand-written macros (§2.3).

### 3.4 P — structural quote patterns, higher-order holes, `TypeMatch` (~520 Rust, ~20 Scala)

dotty's `QuoteMatcher.treeMatch` (`Qi/QuoteMatcher.scala:133-162`) instruments the type bindings
(`:164-186`), then `=?=` (`:209-550`) matches tree against tree under an `Env` of pattern-bound
to scrutinee symbols (`:126-128`): typed holes (`:263-279`, with the varargs hole), higher-order
holes `higherOrderHole[T](args*)` and `…WithTypes` (`:282-314`), ascriptions (`:316-325`),
`Literal` (`:327`), `Ident`/`Select` through `symbolMatch` (`:332-350`, `:557-576`: same symbol,
env, overriding symbols), `Apply`, `TypeApply` (`:352-364`), `Block` extending the env (`:366-383`),
`If`, `WhileDo`, `Assign`, `New`, `This`, `Super`, `SeqLiteral`, type trees by `isSubTypeUnderEnv`
(`:385-433`, `:578-582`), `ValDef`, `DefDef` with parameter clauses (`:435-504`), `Closure` (always),
`NamedArg`, `EmptyTree` (`:505-517`); `Match`, `Try`, `Return` are rejected in patterns
(`Qd/QuotePatterns.scala:59-67`). A result is `MatchResult.ClosedTree` or `OpenTree` (`:608-689`);
an open one becomes a lambda `(x1..xn) => scrutinee` by `newAnonFun` + `Closure` with the captured
references mapped to parameters (`:632-689`) [V]. Buckets [I]: typed holes ~40, HO holes ~135, type
patterns ~65, structural cases ~190, env and plumbing ~120 lines of Scala.

Ours: `Yu/quote_patterns.rs` admits only `[TypeDef*] patternHole[T]` (`:28-31`), the match is a
subtype check of the scrutinee's *type* under a constraint with the bindings as variables
(`Yi/quote_match.rs:79-124`) [V] — the constraint is what dotty's GADT instrumentation does, so it
carries over to a tree matcher that calls `is_subtype_in` at each type position with one
`Constraint` for the whole match. Port, in three MRs:

- **P1 structural (closed) patterns** (~300): `Yu/quote_patterns.rs` keeps the pattern body as
  typed trees (`patternHole` applications stay `TypeApply` nodes; `QuotePattern` gets the root);
  `Yi/quote_match/` gets `normalize` (`Inlined(_, [], _)`, empty blocks), the `Env`, `symbol_match`
  (overriding symbols through `Universe`), the closed-pattern check (free pattern variables), and
  the cases above over `tpd::Tree` (`Block` binds `ValDef`/`DefDef`/`TypeDef` symbols; `DefDef`
  clauses extend the env); the host's `matched` list takes the holes' trees in pattern order
  (`M/host.rs:167-176`). dotty's doc comment `:17-108` is the spec.
- **P2 higher-order holes** (~150): the `higherOrderHole` cases and `OpenTree`: a fresh anonymous
  method symbol with a `MethodType` of the hole's argument types (`PolyType` for `…WithTypes`),
  the scrutinee copied with the captured references replaced (`Yi/copy.rs`'s `TreeTypeMap`) and
  wrapped in `Block(DefDef, Closure)` — the shape `Yi/beta.rs` reduces, so B's `betaReduce` and
  P2 meet in the macros that match `'{ (x: Int) => $f(x) }` and rebuild `'{ $f($y) }`.
- **P3 `TypeMatch` and `'[t]`** (~70): `TypeMatch.unapply(scrutinee: Type[?])(pattern: Type[?])`
  (`Qi/QuotesImpl.scala:3644-3649`) — the pattern type unpickled by `quoted_type` with its
  `@patternType` bindings kept, `bound_types` as today; type-tree positions inside P1's patterns
  use the same path (`'{ $x: List[t] }` already works, `case '[List[t]]` is the type-only form).

Unlocks: P3 — upickle (`'[t]` on every field type), mainargs, tapir's `'[body[bt, cf]]` and the
magnolia-based derivations (zio-json, tapir `Schema.derived`) [V for the use, I that nothing else
blocks them]; P1 — fastparse's `case '{-1}`, then the `run-macros` corpus; P2 — `run-macros` only.
P3 is independent of P1 and comes first.

### 3.5 S — `Type.show`, `TypeRepr.show` (~180 Rust, ~10 Scala)

`Type.show` is `Printer.TypeReprCode.show` → `SourceCode.showType` (`Qi/printers/SourceCode.scala:
14-26`), whose `printType` (`:1096-1271`) with `printRefinement`, bounds, methodic parameters and
`printFullClassName` (`:1272-1440`) is about 300 lines; infix applied types print infix when the
symbol has the `Infix` flag (`:1155`), repeated as `_*`, names fully qualified (`scala.Int`,
`scala.collection.immutable.List[scala.Int]`) [V]. Ours: `crates/tasty/src/types/show.rs` (75
lines) prints `scala.Option[B]`-style text for diagnostics with prefixes as written, `?n` for
variables, `= T` for aliases [V]; the REPL's `:type` goes through `show_type`
(`crates/typer/src/typer/function_types.rs:94`). Port: `types/show_source.rs`, a second printer
after `SourceCode.printType` (the macro output *is* program text or an error message, so the
oracle's spelling matters) wired as a `show` op on `TypeImpl`; `Type.show[T]` in the library is
`Printer.TypeReprCode.show(TypeRepr.of[T])`, so the overlay implements `Printer.TypeReprCode` and
`TypeReprMethods.show`. Unlocks: Circe's `typeName` (error path), upickle (`macros.scala:391`),
mainargs, tapir and pprint's `TPrint` fallback on their success paths, munit's `Clue`
(`TypeTree.show(using Printer.TreeShortCode)`: a type tree prints as its type) [V uses, §2.3].

### 3.6 L — positions and symbol identity (~150 Rust, ~80 Scala)

sourcecode (`Line`, `File`, `Name`, `FullName`, `Enclosing`, `Pkg`, `Text`, `Args`) and munit
(`Location`, `clue`) read `Position.ofMacroExpansion`, `pos.startLine/startColumn/sourceCode`,
`sourceFile.name/path/jpath`, `Symbol.spliceOwner`, `Symbol.owner/name/fullName/flags/isX`,
`Symbol.pos` (`Qi/QuotesImpl.scala:3369-3414` Position and SourceFile, 16 defs; `:2978`) [V defs,
I the libraries — §2.3]. Ours: the splice's span is `MacroCall.span` (`Yi/macros.rs`), spans of
library trees are "in another source" (`crates/typer/src/tpd/print/mod.rs:24-36`), the owner is
`MacroCall.owner`, names and owners live in `Universe::symbol` [V]. Port: ops `expansionPos`,
`pos` (a term's span: start, end, line/column from the `SourceFile`), `sourceCode`, `sourceFile`
(name, path), `owner`, `name`, `fullName`, `flags` (a bit set mapped to dotty's `Flags` in the
overlay), `spliceOwner`, `moduleClass`, `isX` predicates, `defn.RootPackage`; `Position` and
`SourceFile` as overlay classes (`getJPath` as a `java.nio.file.Path` of the script's path).
`Symbol.tree` stays out (definitions as `reflect` trees; sourcecode's `Args` only). Unlocks:
sourcecode's `Line`/`File`/`FileName`/`Name`/`FullName`/`Enclosing`/`Pkg`/`Text`, munit's
`Location` and (with S) `Clue`, scalatest's `Position.here` — i.e. every munit and scalatest
assertion in a script, and kyo's frames [V uses, I that the runtime side of munit/scalatest links;
to probe with `--dep org.scalameta::munit::1.3.6` first].

### 3.7 W — the wider reflect API, in slices

Each a thin op in `M/reflect.rs` + `Yi/reflect.rs` and overlay members, refused by name until it
lands; sized from dotty's regions (`Qi/QuotesImpl.scala`, 871 `def`s) [V regions, I sizes]:

| slice | dotty | ours | Rust | Scala | needed by |
|---|---|---|---|---|---|
| W1 `TypeRepr`: `widen`, `dealias`, `=:=`, `<:<`, `baseType`, `memberType`, `appliedTo`, `select`, `isSingleton`, `baseClasses`, `typeArgs` of aliases; extractors `TypeRef`/`AppliedType`/`RefinedType`/`TypeBounds`/`AnnotatedType`/`TermRef` | `:1871-1989` (32 defs), `:1993-2498` (117) | `Universe::widen/dealias/is_subtype/base_type` exist | ~160 | ~80 | upickle, pprint, magnolia (tapir, zio-json), fastparse's `=:=` |
| W2 `Symbol`: `caseFields`, `primaryConstructor`, `children`, `companionClass`, `fieldMember`, `methodMember(s)`, `paramSymss`, `annotations`, `allOverriddenSymbols`, `requiredClass`, `isX`, `Flags.*` | `:2955-3194` (72 defs, 28 read `denot`), `:2750-2954` | `Universe::decl`, `sealed.rs` children, flags | ~180 | ~80 | upickle, magnolia, mainargs, tapir (`annotations` everywhere) |
| W3 `Implicits.search`, `Expr.summon` | `:2669-2749` | `Yi/compiletime.rs` `summonInline` runs the search through the inlining's host (`Yi/host.rs`) | ~80 | ~30 | mainargs, tapir, jsoniter; `Expr.summon[Encoder[t]]` loops |
| W4 term construction: `Apply`, `TypeApply`, `Select.unique/overloaded`, `New`, `Typed`, `ValDef`/`ValDef.let`, `Block`, `Match`/`CaseDef`/`Wildcard`, `Lambda` (`Symbol.newMethod` + `Closure`, `:924-935`), `Symbol.newVal/newMethod` (`:2916-2925`), `Ident(termRef)` | `:97-1845` | `tpd::Build` has literal, ident, select, type_apply, apply [V partly] | ~300 | ~150 | upickle (`keyToIndex`, enum visitors), fastparse, tapir `CaseClass`, scalatest `assert`, jsoniter |
| W5 `Expr.show` (`Printer.TreeCode`/`TreeShortCode`): `printTree` `:110-546` + helpers `:548-964`; `TreeStructure` (`Extractors.scala`) | ~1,400 lines | `tpd/print/` (407) prints S-expressions for the oracle, not reusable text | ~450 | ~10 | scalatest `assert` messages, utest, os-lib and mainargs error texts, i22649 (`TypeReprStructure`) |
| W6 `report.error/warning/info(msg, pos)` continuing after the message | `:3415-3574` | `MacroError::Failed` only (`Yi/splice.rs:26-29`) | ~40 | ~20 | upickle, quotes-add-erased; every macro that reports and goes on |

### 3.8 Outside macros proper

`scala.compiletime.testing.typeChecks`/`typeCheckErrors` (8 tests of the 12 "scala.compiletime
operation" rows: i7487, i7488, i11630, i12656, i18150, i22968, scala-tests-typeChecks,
typeCheckErrors [V grep]) are dotc intrinsics that type a code string at the call
(`N/Inliner.scala`, `Intrinsics`); ~120 lines parsing and typing the string in a throwaway scope.
`constValueTuple`/`summonAll` rows are stale (both are in `EXPANDED`,
`inline_refs.rs:16,18`, and `Yi/compiletime.rs:58-59` expands them) [I]; i15988a/b are tuple
XXL (research 23 §2.7).

## 4. MRs

Sizes are Rust lines of diff plus overlay Scala [I]; tests are the macro tests of §2 that the MR
is the last blocker of, counted only once T makes them run.

| MR | scope | Rust | Scala | unlocks |
|---|---|---:|---:|---|
| T | §3.1 two-file tests through `lower --cp`, `run-macros` checkout | ~50 (+30 py) | — | 59 "several files" tests of `tests/run` measured (4 macro: 25333 likely passes, i22649 after W5); `run-macros` (~450 [I]) measured |
| L | §3.6 positions, symbol identity | ~150 | ~80 | sourcecode (11 of 12 macros), munit `Location`, scalatest `Position.here`, kyo — every munit/scalatest assertion |
| E | §3.3 term extractors, `Varargs`, `Expr.value` | ~150 | ~180 | os-lib path literals, scalactic `anyvals`, `Expr.ofList` users; fastparse with P1 + W4 |
| P3 | §3.4 `TypeMatch`, `'[t]` | ~70 | ~10 | upickle, mainargs, tapir, magnolia-based derivations (with W1/W2) |
| S | §3.5 `Type.show`, `TypeRepr.show` | ~180 | ~10 | Circe `typeName`, munit `Clue`, pprint; upickle/mainargs/tapir success paths |
| B | §3.2 `Term.betaReduce` | ~80 | ~15 | HO holes in quotes, `Expr.betaReduce` — `run-macros` tests |
| P1 | §3.4 structural closed patterns | ~300 | ~10 | fastparse's literal patterns; `run-macros` (the bulk of its pattern tests [I]) |
| P2 | §3.4 higher-order holes in patterns | ~150 | — | `run-macros` only |
| W1–W6 | §3.7 reflect slices | ~1,210 | ~370 | W1+W2: upickle, pprint, magnolia (tapir, zio-json); W3: mainargs, tapir; W4: fastparse, upickle, tapir; W6: upickle; W5 last |
| C | §3.8 `typeChecks`/`typeCheckErrors` | ~120 | — | 8 `tests/run` tests, munit `compileErrors`, scalatest `assertCompiles` |

Order: T → L → E → P3 → S → B → P1 → P2 → W1 → W2 → W3 → W4 → W6 → W5 → C. T first because
every later count depends on it; L is the cheapest broad win (§2.3); E, P3 and S are each a day
or two and together open the library idioms (extractors, `'[t]`, `show`); B, P1 and P2 are the
one real port (QuoteMatcher, ~550 lines of dotty) and the ROADMAP's named gap — real for
`run-macros`, rare in libraries; W by demand, each slice behind its own refusal names, W5 last; C
is outside macros and goes whenever a coder is free.

Conflicts [V for the files, I for the overlap]:

- `M/reflect.rs` (the op `match`, 117 lines) grows with B, E, S, L, W1–W6: split by subject before
  E (`reflect/terms.rs`, `types.rs`, `symbols.rs`, `positions.rs`) to stay under 200 lines; one
  coder at a time on the dispatch, the per-subject files in parallel.
- `O/ReflectImpl.scala` (200 lines) the same: one trait per `…Module` group (`ReflectTerms`,
  `ReflectTypes`, `ReflectSymbols`), mixed into `ReflectImpl`.
- `Yu/quote_patterns.rs` and `Yi/quote_match.rs`: P1 rewrites both; P2 follows P1; P3 touches
  only `Yu/quotes.rs`'s `quoted_type` and the `TypeMatch` entry, so it can precede P1; E does not
  touch them.
- `Yi/beta.rs`: B calls it, P2 produces its input shape; no edits to it expected.
- `Yi/reflect.rs`'s `Reflect` struct: B adds `fresh`, W3 adds the implicit-search host; L reads
  spans — additive fields, sequential merges.
- Tooling: T edits `scripts/dotty-lower.py` and `lower.rs`, which the typer-group MRs of research 23
  do not; the `run-macros` recipe adds to `Justfile` beside `dotty-sources`.

Risks: P1 changes what a pattern *means* for every macro that matched a typed hole before (the hole
case must keep refusing nothing it accepted — the Circe transcript tests and `tests/macros` guard
it); P2's closures are the first trees a macro gets that it did not quote, so their owners must be
the splice's (`checkEscapedVariables`, `X/Splicer.scala:90-129`) or the lowering sees foreign
symbols; S must match `SourceCode`'s spelling or macro error messages differ from scala-cli's —
compare on `tests/run-macros`' `.check` files once T lands; W4's constructed trees are typed by the
macro, not by our typer (dotty trusts them too), so `retype` (`Yi/retype.rs`) must accept trees it
did not type; `Expr.show` (W5) is the largest and least needed piece — last.
