# 06 — Porting dotty's scanner and parser to Rust

Date: 2026-09-28. Claims are [V] verified by reading the code or running it, or [I] inferred.
Paths are relative to `ref/dotty` (scala/scala3 at tag 3.9.0, sparse: `compiler/src/.../dotc/{parsing,
ast,util,config,printing}`, `tests/{pos,neg}`, `docs/_spec`, `docs/_docs/reference`), with
`P/` = `compiler/src/dotty/tools/dotc/parsing/`. Issue #15; ROADMAP phase 3, step 1.

**Recommendation in one paragraph.** Port `Scanners.scala` (1774 lines) and `Parsers.scala`
(5190) nearly line for line into a new crate `crates/syntax`, producing dotty's *untyped* trees
(`ast/untpd.scala`, `ast/Trees.scala`) in an arena, so that `Desugar` and the typer subset can
later be ported tree for tree. Test every step differentially against dotty itself: a small
Scala program on the `scala3-compiler` 3.9.0 API dumps tokens and trees (with spans) as
S-expressions for all 3967 `tests/pos` and 1932 `tests/neg` files in about 2 s per corpus, and
the Rust side prints the same format. Start with the scanner; its oracle is a free-running
`nextToken()` loop, which dotty's own `ScannerTest` uses too.

## 1. What is normative

- **Grammar of record:** `docs/_docs/reference/syntax.md` (517 lines, "official Scala 3 syntax,
  without deprecated or experimental features"). The spec's `13-syntax-summary.md` is a stale,
  truncated copy (only definitions/templates, old given syntax, a TODO at :103); chapters 05–09
  still carry Scala 2 productions [V].
- **Lexical rules:** `docs/_spec/01-lexical-syntax.md` (rewritten for Scala 3) plus
  `reference/other-new-features/indentation.md`; the executable form is `P/Scanners.scala`.
- **Where they differ, follow the compiler** [V]: form feed is whitespace and a line end, a lone
  CR is neither; identifier classes are Java's `isUnicodeIdentifierStart/Part` (wider than the
  spec's letter classes); `007` lexes as an integer; `"""` strings get no escape processing (the
  spec still claims unicode escapes there; `decodeUni` is dead in `CharArrayReader`); `<` starts
  XML also after `>`; `new` is a `:` predecessor and `throw` an end-marker tag (the reference
  pages are stale on both); trailing commas are dropped only in regions the parser declared
  comma-separated.
- **Parser leniencies are part of the contract**: `Parsers.scala` parses more than the grammar
  and checks afterwards (lambda parameters via `convertToParams`, assignment targets, ascription
  after patterns, `PatDef` without rhs, bounds plus alias on opaque types, refinements through
  `defOrDcl` + `checkLegal`, constructor-annotation heuristics). Desugar and the typer rely on
  the resulting shapes, so the port copies them.
- **Legacy syntax** is decided by `config/MigrationVersion.scala` (warn-from/error-from per
  construct) against the source version (default 3.9): `_` wildcard types, `A with B`, `x: _*`,
  `private[this]`, `= _`, old givens, `implicit` clauses, `import a._` are accepted with a
  warning; procedure syntax, `do … while`, symbol literals, `<%`, `forSome` are errors.

## 2. Scanner (`P/Scanners.scala`, `Tokens.scala`, `CharArrayReader.scala`, `util/Chars.scala`)

- **Tokens** keep dotty's numbering (0–100, `closer = opener + 1` is relied upon) and its token
  sets (`Tokens.scala:152-305`) as `u128` bit sets [V].
- **Pipeline per token** (`nextToken`, :395-403): adjust the region stack for the previous token,
  fetch (or take the one-token `next` buffer), insert NEWLINE/NEWLINES/INDENT/OUTDENT
  (`handleNewLine`, :562-673), post-process (CASE CLASS fusion, `;` before `else`, trailing commas,
  `:` → COLONfollow, `end` markers, closing indented regions) [V].
- **Regions** (`InParens`, `InBraces`, `InString`, `InCase`, `SingleLineLambda`, `Indented` with
  `IndentWidth` = runs of spaces/tabs ordered by prefix) are a stack with `outer` links; the parser
  pushes and rewires some of them. In Rust: an arena of region nodes with parent indices [I].
- **Configuration** is small: indentation on/off (`-no-indent`, `-old-syntax`, 3.0-migration),
  source version, keep comments, `maybeIncomplete`, and the language features the scanner reads
  (`erasedDefinitions`, `modularity`, `postfixOps`, capture checking) — the parser changes the
  latter at `import language.…` [V].
- **Offsets:** dotty counts UTF-16 units; the port scans `&str` with byte offsets and the oracle
  maps UTF-16 indices to UTF-8 byte offsets before printing [I].
- **Driven by the parser:** COLONeol (`observeColonEOL`), `InCase`/condition/`for` regions,
  `observeIndented/Outdented`, `commasExpected`, and token rewrites (`SELFARROW`, end markers) only
  happen under the parser. A free-running loop still reproduces lexing, NEWLINE rules, INDENT
  after `=`/`=>`/`then`/…, OUTDENT cascades, leading infix operators, strings and comments [V].
  So the scanner is tested in two stages: free-running now, parser-driven with the parser.

## 3. Parser and trees (`P/Parsers.scala`, `ast/untpd.scala`, `ast/Trees.scala`)

- **Sections** (line ranges in `Parsers.scala`): token classes 217-274, error handling 276-477,
  combinators 630-741, lookahead 968-1175, operator stack 1176-1292, identifiers and literals
  1293-1549, newlines and end markers 1550-1656, types 1657-2412, expressions 2413-3346, patterns
  3347-3517, modifiers 3518-3670, parameters 3671-3965, definitions 3966-4791, templates
  4792-4915, statement sequences 4916-5169. XML (599-629) and rewrites (742-967) are not ported [V].
- **Infix operators** go through one shunting-yard (`infixOps`/`reduceStack`) shared by
  expressions, types and patterns; precedence by first character (`parsing/package.scala:17-38`),
  right-associative iff the name ends in `:`, mixing associativities at one precedence is an error.
  `=>`, `?=>`, `=>>` are tokens handled above the infix level [V].
- **The parser does not desugar**: it keeps `InfixOp`/`PrefixOp`/`PostfixOp`, `Parens`, `Tuple`,
  `ForYield`/`ForDo` with `GenFrom`/`GenAlias`, `InterpolatedString`, `PatDef`, `ExtMethods`,
  `Number(digits, kind)`. The only rewrites are `A with B` → `&`, `new C(a)` →
  `Apply(Select(New(C), <init>), a)`, givens → `ModuleDef`/`TypeDef`/`ValDef`/`DefDef`, anonymous
  `using` parameters → `x$n`, `_` placeholders → `WildcardFunction`, `def this` → `DefDef(<init>)`
  [V]. `Desugar.scala` (2624 lines) runs later, from Namer and Typer.
- **Tree design for Rust:** one `NodeKind` enum (~70 variants) for terms, types and patterns, as
  dotty shares most nodes across the three and decides by namespace or by mode later; nodes in
  an arena with `NodeId(NonZeroU32)`, `Option<NodeId>` for `EmptyTree`, child lists as slices of a
  shared pool, interned names with a term/type bit, a `Modifiers` side arena, attachments
  (`Backquoted`, `HasEndMarker`, using-apply) as node flags [I].
- **Spans** are dotty's packed `start/end/point` with source-derived vs synthetic spans and the
  envelope rule of `Positioned` (a node without an explicit span covers its children). Error
  positions and the oracle depend on reproducing it exactly [V].
- **Gating:** source-version switches (fewer braces 3.3, new givens and clause interleaving 3.6,
  named tuples 3.7, better fors 3.8) and language features; experimental syntax (capture
  checking, erased, sub-cases, relaxed lambdas, multi-spreads) is left out at first and reported
  as a syntax error [I].

Estimated size: scanner 2.1–2.6k lines, parser 5.5–7k, trees/names/spans/flags/printer
1.5–2k, diagnostics 0.5k [I].

## 4. Differential testing

- **Oracle** (`scripts/syntax-oracle/Oracle.scala`, mode `trees`) [V]: a scala-cli program
  on `scala3-compiler` 3.9.0 that initializes one `ContextBase` (277 ms; the parser needs
  `defn.andType` for `A with B`), then per file sets that file's `//> using options`, parses
  with `new Parsers.Parser(source).parse()` under a `CompilationUnit`, and prints the tree as an
  S-expression with spans and modifiers plus the diagnostics (reporter with
  `UniqueMessagePositions`, as dotty's test framework). All of `tests/pos` in 1.9 s, `tests/neg`
  in 1.1 s, in one JVM. `-Vprint:parser` is no alternative: it prints source-like text without
  node kinds or offsets.
- **Tokens:** the same program in mode `tokens` prints `kind offset lastOffset lineOffset name`
  per token of a free-running scanner.
- **Corpora:** `tests/pos` (3967 files; 256 with `//> using options`) and `tests/neg` (1932
  files; 254 of them get a parser error, about 164 only parser errors). Pos files must match
  byte for byte; neg files must report errors at the same offsets. Skip `-Ymagic-offset-header`
  files and `.java` sources.
- **Harness:** `scripts/syntax-tests.py tokens|trees`, results under `tmp/syntax/3.9.0/<mode>`,
  recipes `syntax-oracle`, `syntax-report` and `syntax-diff`.

## 5. Order of work

1. Crate skeleton: source files and spans, names, token table and sets, diagnostics.
2. Scanner without parser-driven regions; token oracle on both corpora.
3. Trees, S-expression printer, tree oracle.
4. Parser: expressions → types → patterns → definitions and parameters → templates and packages
   → Scala 3 additions (enums, givens, extensions, export, colon arguments) → error recovery and
   `tests/neg` parity. Parser-driven scanner features arrive with the parser methods that use them.
5. Later, separate issues: `Desugar`, symbols from TASTy, the typer subset.
