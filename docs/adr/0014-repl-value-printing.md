# ADR 0014 — REPL values printed by a pprint port in the overlay

Status: accepted · Date: 2026-10-07

## Context
Dotty's REPL prints values with its vendored copy of Li Haoyi's pprint: by runtime class,
wrapped at the page width, cut off after 50 lines, case classes with field names, a product's
own `toString` kept. The REPL first printed `ScalaRunTime.stringOf` and quoted by static type
(ADR 0012), which differs for products, nested strings and values typed `Any`. Two of pprint's
inputs are not plain Scala: `collectionClassName` is private to package `scala`, and whether a
`toString` is the one a case class is given needs its compiled body.

## Decision
- **Port:** `overlay/src/scalai/repl` holds pprint's walker, renderer and truncation in Scala,
  without colors: `scalai.repl.Show(value, width, height, offset)`. A helper class of the
  overlay (ADR 0009), compiled to NIR and embedded.
- **Two natives** (`crates/vm/src/natives/repl.rs`) for the methods `Show` cannot write:
  `hasUserToString` resolves the class's `toString` and answers false for `Object`'s and for a
  body that only returns `ScalaRunTime._toString(this)`, as dotty's `ReplBytecodeAnalysis`
  does on bytecode; `collectionName` calls `collectionClassName`.
- **Call:** each value's show method is `def x$show: Any = x`, since the typer only knows the
  library's TASTy; `Vm::show_value` calls it and passes the result to `Show` in one run. The
  offset is the length of the definition's last line plus ` = `; the width is `$COLUMNS` or 80.
- `rs$line$N` prefixes are stripped from the result in Rust, as dotty's `stripReplPrefix`.

## Consequences
- Output matches dotty's for what the frontend compiles, including wrapping and truncation;
  pprint's laziness keeps cyclic collections finite.
- A walker in Rust would have to call `toString`, `iterator` and the product methods through
  the interpreter one by one and copy pprint's logic in another language; the Scala port stays
  a near line-by-line copy of pprint.
- Deliberate differences from dotty:
  - a trace thrown while printing stops at the first `scalai.repl` frame and counts the rest as
    elided, where dotty shows pprint's and its reflection frames;
  - `hasUserToString` only reads the compiled body, where dotty first asks the class's symbol:
    a written `toString = ScalaRunTime._toString(this)` counts as synthesized;
  - ANSI escapes in a `toString` are printed as they are and count towards the width, where
    fansi parses them out;
  - a `$COLUMNS` that is not a positive number gives 80, where dotty fails to start or takes
    it as the width.
