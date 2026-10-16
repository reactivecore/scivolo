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
- **Handles, not objects.** `Expr` and `Type` are the overlay's `ExprImpl(id, expansion)`
  and `TypeImpl(id, expansion)` in `scala.quoted.runtime.impl` (the compiler's package, for
  the `private[scala]` constructors): numbers into the frontend's table of trees (`TNodeId`)
  and types (`TypeId`) of the expansion numbered `expansion`. A handle a macro kept from an
  earlier expansion refuses the expansion that uses it (dotty's scope extrusion check).
- **No VM addresses in Rust.** The implementation's arguments are made in Scala
  (`QuoteHost.args()`, from the numbers a native hands out) and read from that array straight
  into the call, so Rust holds no object across an allocation and the collector needs no new
  roots.
- **Every refusal counts.** `Unsupported`'s constructor records its reason on the host
  (native `QuoteHost.refuse`), and natives that refuse or panic throw it; an expansion with a
  recorded refusal is refused whatever the macro returns, also when it caught the exception.
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
- Macro side effects happen in the VM that compiles the script. So that they do not reach
  the program, as they would not on the JVM, scalai runs itself again in place of its process
  (`exec`, with `SCALAI_RERUN` set) after a compile that ran a macro, and the program runs from
  the cache in a fresh VM. Output a macro buffered in the VM without flushing is lost.
- Scripts read the TASTy of `--cp` and `--dep` jars in the own frontend, with or without
  `SCALAI_MACROS`, as the REPL does (most of #128); the script cache key holds each classpath
  entry's path, size and time.
- The TASTy of every quote unpickled stays loaded in the universe (`Universe::load_quote`),
  since the symbols read from it point into it: a script pays per expansion, the REPL for the
  session.
- Literal quotes such as `'{ 42 }` compile to `ToExpr`, which uses `quotes.reflect`, so they wait
  for M4b.
- Rejected: a second VM for macros (one Immix per process); Rust natives for all of `Quotes`
  (the hole closures would make natives re-enter the interpreter).
