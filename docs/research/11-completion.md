# 11 — REPL tab completion

Date: 2026-10-09. Claims are [V] verified by reading the code or running it, or [I] inferred.
Paths: `C/` = `ref/dotty/compiler/src/dotty/tools/dotc/`, `R/` = `ref/scala3-repl-3.9.0/dotty/
tools/repl/`. Issue #69; the policy is [ADR 0018](../adr/0018-repl-completion.md). Oracle answers
are those of scala3-repl 3.9.0 and JLine 4.0.14 on JDK 21.

**Recommendation.** Port `Completion.scala` as a computation of label sets (the REPL shows
labels only, deduplicated, no types), driven by a *probe* in our typer: the input is parsed
leniently, wrapped like `:type`, and typed until the tree under the cursor; the typer records
the context there and the qualifier of a selection, and returns an error tree instead of
looking the name up. Candidates come from the existing member and scope machinery with dotty's
validity filter.

## 1. dotty's algorithm

### 1.1 From the key press to `Completion.completions`

1. JLine asks the REPL's parser for the word under the cursor (`R/JLineTerminal.scala:220-292`)
   [V]: the whole input if it is a prefix of a command, else the identifier or keyword token
   containing the cursor (an unclosed backtick keeps its start), else an empty word. `LIST_MAX`
   is 400; Tab inserts a tab when the buffer's last line is blank (`insertTab`).
2. The completer (`R/ReplDriver.scala:184-211`) [V] adds one candidate per distinct label:
   value the label, display the label without backticks, no description, no space appended.
   Candidates whose label equals the word print their `showUser` signature above the prompt
   (not ported, ADR 0018).
3. `completions` (`R/ReplDriver.scala:341-363`) [V]: an input starting with `:` completes the
   15 commands (`R/ParseResult.scala:169-185`) by prefix and nothing else. Otherwise the input is
   typed with errors allowed (`R/ReplCompiler.scala:208-282`): parsed as `blockStatSeq` even on
   syntax errors, wrapped as `final class $wrapper { val expr = { <stats>; () } }` (the input's
   definitions are *block locals*), compiled to the typer, then `Completion.completions` runs at
   the cursor; an exception gives one bogus candidate.
4. `Completion.completions` (`C/interactive/Completion.scala:65-73`) [V] takes the typed path to
   the cursor (`Interactive.pathTo`, innermost first, recovery trees skipped), rebuilds the
   context along it (`Interactive.contextOfPath`: imports before the cursor, parameters, block
   locals, case binds, class bodies), and reads mode and prefix off the *untyped* path.
5. JLine filters the candidates by the word of step 1 (`LineReaderImpl.doComplete`, Tab being
   `complete-word`; `CompletionMatcherImpl.defaultMatchers`) [V]: the values starting with the
   whole word; if none, those containing it; if none, the typo fallback: those whose
   `ReaderUtils.distance` to it is below 2, plus, when there are several, the word itself in
   the group `original` (`x.ma@@q` lists `maq` above the `ma…` members). The word can differ
   from dotty's prefix, which ends at the cursor: `x.ma@@p` shows `map mapConserve` only. One
   match is inserted; else the common prefix, then the list (`computePost`, `GROUP` and
   `AUTO_GROUP` on): with one group, no header; with several, `original` first and the REPL's
   ungrouped labels under `others`, each under its header; within a group sorted by
   `Candidate.compareTo` (`String.compareTo` on the value).

### 1.2 Mode and prefix (`Completion.scala:102-168`) [V]

| untyped path head | mode |
|---|---|
| `Select`/`Ident` in a package clause | none |
| import selector, on the imported name (or a `given` bound) | import (terms and types) |
| import selector, on the rename | none |
| `Ident` directly under `Import`/`Export`, or the import itself | import \| scope |
| string literal, interpolated string | term \| scope |
| `RefTree` with a term name | term, plus member for `Select`, scope for `Ident` |
| `RefTree` with a type name | type, plus member/scope likewise |
| anything else (keyword, literal, after `=`, inside `f(…)`) | none: no candidates |

No keywords, no named arguments. `new Foo` excludes enums. The prefix is the name up to the
cursor (a leading backtick dropped); for the parser's `<error>` name (`x.`) it is empty, or the
text from a leading backtick. Matching is `startsWith`.

### 1.3 Candidates (`Completion.scala:234-752`) [V]

- **Selection** on a qualifier: a package gives its direct members; a value type gives
  conversion members, then extensions, then direct members, later ones winning per name.
- **Direct members** (`accessibleMembers`): every member name of the site that is not a
  constructor, has a simple name kind (no `$default$`, `x$1`, expanded or super-accessor names)
  and matches; each alternative kept if valid and accessible from the site (private and
  protected members of an outside type drop out).
- **Valid** (`isValidCompletionSymbol`, :355-377): exists, not a primary constructor, has a source
  symbol, a package only as its module class, no setter, no package object, no `Artifact`; term
  mode takes terms and module classes, type mode types and stable terms. Deprecated and
  specialized names stay (`_1$mcI$sp`).
- **Scope**: the context chain outward; an import contributes its imported names (renames,
  exclusions, givens), a class its accessible members, a scope its symbols. Per name: the
  innermost definition wins if in the innermost entry's scope; imports alone resolve to the
  innermost, or by `Predef` before `scala` before `java.lang` if at most one is not a default
  import; else the name is ambiguous and dropped.
- **Conversions**: every implicit conversion applicable to the qualifier, members of each target.
- **Extensions**: the four ways one can apply (in scope, member of a given in scope, implicit
  scope of the qualifier's type, member of a given there), each kept only if applying succeeds.

### 1.4 Labels and order

The result is a map by name, unordered [V]. A label that is not a plain identifier or operator,
or is a keyword, is backticked, and every label is when the prefix starts with a backtick.

## 2. What the corpus shows [V]

The answers are in `tests/repl-completion/*.tab`; the points they make:

1. JDK dependence: `java.lang` classes and their nested `$` classes in scope (`Li` gives
   `LiveStackFrame$PrimitiveSlot`, `Str` gives `StringTemplate$Processor$Linkage`), JDK root
   packages (`sun`, `javax`), JDK members of `String`, `Integer` (`describeConstable`) and
   `ArrayList` (`addFirst`).
2. `(1, 2)._`: dotty returns every member (`_` scans as a wildcard token, so the name is
   `<error>` and the prefix empty), and JLine's word `_` keeps `_1`, `_2` and their
   specializations. `x.hea@@x` and `x.ma@@q` take the typo fallback of §1.1 step 5.
3. `1.` offers `isNaN`, `floor`, `toDegrees`; `x.size.` does not.
4. `1 -` offers `-` and `->`: an infix operator completes as a member of its left operand.
5. Nothing after a command (`:type x.`), on a keyword (`case`), on a rename, a type projection
   (`Box[Int]#El`), an enum after `new`, an extension of an object not imported.
6. An error elsewhere in the input does not stop completion (`val q: Int = "s"; x.hea`).
   Inside a string literal, scope names complete: `"pri` and `s"pri` give `print printf println`.
7. `x.\`hea` gives `` `headOption` `` before `` `head` ``: sorted with backticks.

## 3. What we need

### 3.1 Parser and typer

Our parser already leaves dotty's shapes for unfinished input: `x.` is `Select(x, <error>)` with
the point at the name, `import a.` an `<error>` selector, `{Ar` a selector `Ar` [V]. Needed: a
lenient parse beside `input::classify` that keeps statements and diagnostics. The typer gives no
result on an unsupported construct or a missing member, so completion cannot read `type_unit`'s
output: a probe (target node, recorded context, qualifier type, import qualifier) fires in
`typed`, `typed_select`, `typed_ident` and `import_info`, and returns an error tree at the target.
Candidates are computed in the same `Typer`; the session's `Units` stay as they were.

### 3.2 When to show nothing

- The probe never reached the target: dotty's mode none and our safety net.
- The qualifier is an error tree, or its widened type holds `Type::Error` or `Type::Var`.
- An unsupported construct without position, or at or before the cursor.
- A site whose member set the universe cannot know: what `did_you_mean::member_names` refuses
  (a JDK package other than `java.lang`, a JDK class without a member table, a missing base
  class, a self type, an export clause).
- `java.lang` names: only those in javalib and `JAVA_LANG`, nested `$` classes never.
- A panic: caught, no session reset.

Derived names need a flag on `Symbol` set from the TASTy name kind, not a `$` pattern, which
would drop `_1$mcI$sp`.

## 4. The oracle and the corpus

`just completion-oracle [files]` (`scripts/completion-oracle/`) rewrites corpus files in place
with what the REPL lists: the labels of `ReplDriver.completions`, filtered, grouped and sorted by
JLine's own code (the REPL's `JLineTerminal` parser for the word, a `LineReaderImpl` with the
REPL's options for its matcher and `computePost`). One fresh `ReplDriver` per file, its classpath
cut to the Scala library jars so that no tool package (`xsbti`) leaks in. It refuses a JDK other
than 21. A `scala>` line that fails to compile or throws, a `tab>` line without `@@`, dotty's
error candidate, a bad `miss>` line or an unexpected line leaves its file unwritten, the other
files are still checked, and the exit code is non-zero. The whole
corpus takes 6–20 s, plus about 6 s for the scala-cli start. CI does not run it.

A `.tab` file holds lines of these kinds:

| line | meaning |
|---|---|
| `// oracle: …` | written by the oracle: versions it ran on |
| `// …`, empty | kept |
| `scala> input`, `     \| more` | run in the session |
| `tab> text`, `     \| more` | ask at `@@`, which is removed |
| two spaces, labels | the answer, written by the oracle: the labels listed, in JLine's order, space-separated, wrapped at 100 columns, backticked labels as one token; each group with a header starts a line with `[header]` (§1.1 step 5); `<none>` |
| `miss> labels` | right after an answer (comment lines between allowed), by hand, with a `//` reason: labels we knowingly do not offer; the oracle checks that the answer has them |

Our test (stage 2) runs the same file through `Session` and the word filter: it compares
sequences, expecting exactly the answer less the misses, in the same order.

## 5. Stages

1. The oracle, the corpus, this document, ADR 0018.
2. Typer probe and selection completions (direct members): `Symbol` derived flag, path to the
   cursor, member enumeration and validity filter, `Session::complete`, the corpus test, the
   measurement of §6.
3. The rustyline completer: JLine's word filter (§1.1 step 5; rustyline has none), `Pair`
   candidates (display without backticks), list mode, prompt limit 400, commands; piped mode
   unchanged.
4. Scope, import and type-position completions, with the shadowing resolution and `ScopeOrdering`;
   `java.lang` names from javalib ∩ `JAVA_LANG`; root packages.
5. Conversions and extensions, each applied before it is offered; the JDK member table of
   ADR 0018 and its generator.

## 6. Cost of a wrapper per Tab

The wrapper is a fresh `rs$line$tab$K` object, never run nor imported. Stage 2 measures the
latency of `x.toString.` style inputs and the symbols added per Tab.
