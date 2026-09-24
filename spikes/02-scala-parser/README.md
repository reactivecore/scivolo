# Spike 02: Scala 3 parser for a Rust frontend

Question: should the interpreter's own frontend (parser + typer subset over library TASTy) parse
with **tree-sitter-scala**, or with a **hand-written Rust port of dotty's parser**?

Measured on 2026-09-24, Apple M4 Pro, `cargo build --release`, tree-sitter 0.26 +
tree-sitter-scala 0.26.2, Scala 3.9.0 corpora.

## Commands

```sh
cargo build --release
./fetch-corpus.sh /tmp/corpus    # scala-library-3.9.0-sources.jar + sparse scala3 clone at tag 3.9.0
B=target/release/scala-parser-spike
C="/tmp/corpus/stdlib /tmp/corpus/scala3/tests/run /tmp/corpus/scala3/tests/pos
   /tmp/corpus/scala3/compiler/src ../00-baseline/scripts features"
$B ts   --iters 10 --dump ts-failures.tsv $C   # ERROR/MISSING census + tree-sitter MB/s
$B lex  --iters 50 --dump lex-errors.tsv  $C   # hand-written lexer MB/s
$B nest --dump nest.tsv $C                     # silent misnesting check (see below)
$B sexp --dump trees.txt features/quotes.scala # dump tree-sitter trees
$B tokens --dump tokens.txt features/misc.scala
```

`features/*.scala` are small files exercising Scala 3 syntax (indentation syntax, end markers,
fewer-braces lambdas, given/using, extension, enums, match types, type lambdas, polymorphic
functions, opaque/union/intersection types, named tuples, quotes/splices, inline match, export,
`tracked`, interpolation, trailing commas). All pass dotty 3.9.0 (`scala-cli compile -experimental`).

## Results

| corpus | files | MB | files w/ ERROR/MISSING | tree-sitter parse | parse + full walk | lexer | tokens |
|---|---:|---:|---:|---:|---:|---:|---:|
| stdlib sources 3.9.0 | 649 | 4.57 | 0 | 12.6 MB/s | 11.2 MB/s | 273 MB/s | 645 k |
| dotty `tests/run` (all) | 1930 | 2.63 | 15 (0.8 %) | 9.2 MB/s | 8.2 MB/s | 197 MB/s | 592 k |
| dotty `tests/pos` (all) | 3967 | 3.49 | 40 (1.0 %) | 6.1 MB/s | 5.4 MB/s | 151 MB/s | 1.10 M |
| dotty `compiler/src` | 489 | 7.63 | 0 | 12.5 MB/s | 11.0 MB/s | 222 MB/s | 1.23 M |
| `00-baseline/scripts` | 3 | 0.004 | 0 | 7.9 MB/s | 6.9 MB/s | 163 MB/s | 919 |
| `features/` | 8 | 0.01 | 0 | 5.9 MB/s | 5.2 MB/s | 125 MB/s | 2 k |

Small-file corpora are slower per byte (per-file setup). The lexer reports no lexical errors on
any corpus; `tokens` output was checked by hand on `features/`.

### tree-sitter failures, all 55 failing files categorized

Experimental / flag-gated syntax, 27 files:

| feature | files |
|---|---:|
| `erased` parameters and `(erased T) => R` types | 16 |
| sub-cases `case p if x match ...` (`experimental.subCases`) | 6 |
| `relaxedLambdaSyntax` (`= case ...`, `.collect: case x =>`) | 3 |
| capture checking `->{xs*}` | 1 |
| applied constructor types `Person("K")(27)` (modularity) | 1 |

Stable Scala 3 syntax, 28 files:

| construct | files |
|---|---:|
| indentation edge cases: `throw` + indented block, `if (c)`/`while (c)` + indented body, multi-line `if` condition, guard ending in `:` block, ascription line in indented block, `-no-indent` | 6 |
| Scala 2 / migration-mode syntax (`do {} while`, `implicit (x: T) =>`, `-Xkind-projector`, postfix `v<1> = 10`) | 4 |
| `*` as infix operator in types and patterns (`a.type * b.type`, `P(a, b * c)`) | 3 |
| leading infix operators on continuation lines | 2 |
| polymorphic lambdas/types with higher-kinded params, `[T] => (T => T)`, `[t] =>` as argument | 2 |
| annotated constructors `class C @ann private[x] (...)` | 2 |
| backquoted identifiers (`` `t1` <- `` in `for`, `` `i'` += 1 ``) | 2 |
| `end new` | 1 |
| type lambda as wildcard bound `? <: [X] =>> ...` | 1 |
| class type parameters on the next line | 1 |
| `enum E` with `{` on the next line | 1 |
| trailing comma in `( 23, )` | 1 |
| stray `; ;` | 1 |
| object named `+` (`+.x`) | 1 |

### Silent divergences (no ERROR node, wrong tree)

`nest` flags definitions whose tree-sitter parent disagrees with the source indentation. After
removing irregular-indentation false positives (3-/5-space members after doc comments), it finds 0
in stdlib and `compiler/src`, none beyond the ERROR files in `tests/run`, and 2 real cases in
`tests/pos`. Probing by hand found more:

- **Infix types and patterns are flat left-associative.** `case a :: b :: rest` parses as
  `(a :: b) :: rest`, `Int *: String *: EmptyTuple` as `(Int *: String) *: EmptyTuple`,
  `A | B & C` as `(A | B) & C`. Expressions follow SLS 6.12.3 correctly.
- **Splices `$x` inside quotes are identifiers**; only `${ ... }` is a `splice_expression`.
- **Empty indented template followed by an end marker** (`class D:` / `end D`) swallows every
  following definition into `D` (tests/pos i10080, i24250).
- Named-tuple values are `tuple_expression(assignment_expression ...)`; the consumer decides.

## Hand-written parser: size and cost

The lexer here (`src/lex/`, ~530 lines after rustfmt: identifiers, operators, keywords, number/char/string
literals, nested comments, `s"..${..}.."` interpolation with nested splices, `'{`/`'x` quotes,
INDENT/OUTDENT/NEWLINE from a region stack) runs at **150–270 MB/s**, 20× tree-sitter. It is a
throughput ceiling, not a conforming scanner: NEWLINE insertion and region rules are simplified.
A recursive-descent parser building an arena AST usually costs 1–2× the lexer, so a ported parser
lands at an estimated **50–120 MB/s**, 5–10× tree-sitter. Neither matters much for scripts: a
20 KB script parses in ~2 ms with tree-sitter and ~0.2–0.4 ms hand-written; typing against TASTy
dominates.

Reference sizes at tag 3.9.0 (`compiler/src/dotty/tools/dotc`):

| file | lines | code lines |
|---|---:|---:|
| `parsing/Parsers.scala` | 5190 | ~3900 (422 `def`s, 107 syntax-error sites) |
| `parsing/Scanners.scala` | 1774 | ~1360 |
| `parsing/Tokens.scala`, `CharArrayReader.scala` | 443 | |
| `ast/untpd.scala` (untyped trees) | 911 | |
| `ast/Desugar.scala` | 2624 | needed with either parser |
| `parsing/xml/*` | 1184 | skip (XML literals) |
| tree-sitter-scala `grammar.js` + `scanner.c` | 3300 + 1909 | for comparison |

The reference grammar (`docs/_docs/reference/syntax.md`, 152 productions) is not sufficient on
its own: the NEWLINE/INDENT rules, operator reassociation, `sourceVersion` and language-import
gates (`erased`, `tracked`, `subCases`, `-no-indent`) live in the code.

Estimate for a Rust port of Scanners + Parsers to dotty parity (without XML and Java parsers):
lexer ~1.5–2 k Rust lines (1–2 weeks), parser ~6–8 k lines (5–7 weeks), untyped AST with spans
~1.5 k lines (1 week), and a differential harness that compares against dotty
`-Ystop-after:parser` / `-Vprint:parser` on `tests/pos` and parse errors on `tests/neg`
(2–3 weeks): **about 2.5–3.5 person-months**. The port is mechanical: Parsers.scala is a
one-token-lookahead recursive descent with explicit lookahead helpers, which maps 1:1 onto Rust.

## Recommendation

**Hand-port dotty's Scanners/Parsers to Rust as the interpreter frontend; keep tree-sitter for
editor tooling only.**

- tree-sitter-scala is far better than expected: 0 errors on the 12 MB of stdlib and compiler
  sources and ~1 % on dotty's edge-case tests. It is a good fit for highlighting, outlines and
  fast "is this well-formed" checks.
- It is not a fit for exact semantics. The divergences that matter are silent: wrong
  associativity of every infix type and pattern (`::`, `*:`, `|`/`&`), `$x` splices as names, a
  swallowed-definitions bug. Each needs a fix-up pass or a grammar patch in JS + a C external
  scanner, maintained against a moving upstream with its own design.
- Error messages: tree-sitter yields ERROR/MISSING nodes with no expected-token information;
  dotty-quality messages ("`:` expected, but `val` found") come from Parsers.scala's control flow.
  A port reproduces them and their positions.
- A CST-to-AST lowering over ~150 node kinds is needed anyway with tree-sitter; with a port the
  parser emits dotty's untyped trees directly, so `Desugar.scala` (for-comprehensions, enums,
  case classes, context bounds, givens) and later the typer port line up tree-for-tree.
- Language imports and `-source` change parsing mid-file; dotty's parser models that, a
  context-free grammar cannot.
- The port is 5–10× faster and has no C dependency; the C runtime plus a 27 MB generated
  `parser.c` goes away.
