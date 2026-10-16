# ADR 0029 — Macros run in the VM, quotes are handles to the typer's trees

Status: accepted · Date: 2026-10-16

## Context
A Scala 3 macro call `${ impl('x) }` expands while inlining: dotty's `Splicer` interprets the
splice, calls `impl` as compiled code with `Expr`s, `Type`s and a `Quotes` that wrap its own
trees and types, and splices the tree `impl` returns. A library's macro implementation exists
as NIR only (the `--dep` jar's TASTy holds its signature, not code we can interpret), and its
quotes are pickled TASTy unpickled through `QuoteUnpickler.unpickleExprV2` (research 25 §2.4).
The typer has no VM; scripts created theirs after compiling; Immix allows one VM per process
(ADR 0004).

## Decision
- **One VM.** The script path creates its VM before compiling and adds the compiled classes
  to its classpath afterwards; the REPL uses its session's VM. Macro implementations run in
  that VM on a main green thread of their own (`Vm::expand_macro`), before the script's
  runtime is initialized.
- **Handles, not objects.** `Expr` and `Type` are the overlay's `ExprImpl(id)` and
  `TypeImpl(id)` in `scala.quoted.runtime.impl` (the compiler's package, for the
  `private[scala]` constructors): numbers into the frontend's table of trees (`TNodeId`) and
  types (`TypeId`) for one expansion. No VM object is held outside the VM, so the collector
  needs no new roots; a handle outlives its expansion only as a dead number.
- **Where Quotes lives.** `QuotesImpl` is Scala in the overlay: `unpickleExprV2` and
  `unpickleTypeV2` call the Rust natives of `QuoteHost` (unpickle, list the term holes, fill
  a hole), and run the term-hole closures themselves, so natives never call back into Scala.
  Every other member throws `Unsupported(<member>)`; `reflect`'s getter is a native that does,
  since the trait's `val` cannot be implemented lazily. The natives reach the frontend through
  `vm::QuoteHost`, a trait object the VM holds only while a macro runs.
- **Lifetime of the typer's state.** The typer lends its universe, trees and names to the
  expansion (`typer::inlining::Macros`); the `macros` crate moves them into the `QuoteHost`
  for the run and back afterwards, as the typer lends them to its own implicit searches.
- **Splicing stays in the typer.** The host only records quotes (tree, spliced type symbols,
  hole fillings); `inlining/splice.rs` builds the expansion by copying each quote with its
  holes filled and its spliced types replaced, then retypes it, which expands the inline calls
  in it (dotty's `typedExpr` after `expandMacro`).
- **Behind `SCALAI_MACROS=on`** until quote patterns and the reflect subset (M4b) pass
  `typer-compare`; without it, and without a VM (typer reports, AOT), macro calls stay refused.

## Consequences
- A macro whose implementation is not on the VM's classpath, or that uses `quotes.reflect`, a
  quote pattern or `Expr.show`, is refused by name and the script falls back to scala-cli; an
  exception the implementation throws is dotty's "Failed to evaluate macro" error.
- Macro side effects happen in the VM that then runs the script, as they would not on the JVM;
  the runtime's initialization comes after them.
- Literal quotes such as `'{ 42 }` compile to `ToExpr`, which uses `quotes.reflect`, so they wait
  for M4b.
- Rejected: a second VM for macros (one Immix per process); Rust natives for all of `Quotes`
  (the hole closures would make natives re-enter the interpreter).
