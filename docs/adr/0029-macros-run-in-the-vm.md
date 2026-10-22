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
  into the call, the array kept live on the native stack until the call returns, so Rust holds
  no object only in its heap across an allocation and the collector needs no new roots.
- **Every refusal counts.** `Unsupported`'s constructor records its reason on the host
  (native `QuoteHost.refuse`), and natives that refuse or panic throw it; an expansion with a
  recorded refusal is refused whatever the macro returns, also when it caught the exception.
- **Where Quotes lives.** `QuotesImpl` is Scala in the overlay: `unpickleExprV2` and
  `unpickleTypeV2` call the Rust natives of `QuoteHost` (unpickle, list the term holes, fill
  a hole), and run the term-hole closures themselves, so natives never call back into Scala.
  Every other member throws `Unsupported(<member>)`. The natives reach the frontend through
  `vm::QuoteHost`, a trait object the VM holds only while a macro runs.
- **Quote patterns** of a typed term hole (`'{ $m: T { type L = l } }`, dotty's `QuoteMatcher`
  hole cases) are matched by the typer (`inlining::quote_match`): the pattern's type bindings
  become type variables of a subtype check of the scrutinee's type against the hole's, and are
  approximated after it; other pattern shapes are refused by name.
- **`quotes.reflect`** is the overlay's abstract `ReflectImpl` with the members macros such as
  Circe's use (terms as `ExprImpl`s, `TypeRepr`s as `TypeImpl`s, symbols by number), each a
  named operation the typer runs (`inlining::Reflect`). `reflect`'s getter and the `given`
  method objects are allocated by a native without a constructor, since the traits' other
  members stay unimplemented; calling one is a VM error naming the member, which refuses the
  expansion.
- **Symbol flags** are the TASTy modifier bits as dotty's `TreeUnpickler` reads them, with a few
  bits for flags from the kind (`Method`, `Package`, `Param`) or the file (`Scala2x`); flags TASTy
  does not carry (`Deferred`, `ParamAccessor`, `NoInits`) or dotty sets while typing
  (`StableRealizable`) are refused by name. dotty's symbols the
  universe lacks are made per expansion and entered in no scope: the splice's owner `macro`
  (`Splicer.splice`) and the empty package, owner of the root's definitions.
- **Term extractors** read a tree through one host operation, its kind and the numbers of its
  children in the extractor's order; the overlay's `TypeTest`s and `unapply`s are written over
  it. Trees a macro builds take the expansion's span (dotty's `withDefaultPos`), and a hole
  filled with a tree of the call's source takes the hole's span (`PickledQuotes.spliceTerms`).
- **`report`**: errors are recorded on the host with their spans and reported for the expansion
  (`errorAndAbort` throws `StopMacroExpansion` after recording); warnings and infos are refused.
  Type checks (`=:=`, `<:<`, `isExprOf`) run without type variables; a `false` on types the
  universe may not decide (type variables, wildcards, match types, unresolved references) is
  refused.
- **Lifetime of the typer's state.** The typer lends its universe, trees and names to the
  expansion (`typer::inlining::Macros`); the `macros` crate moves them into the `QuoteHost`
  for the run and back afterwards, as the typer lends them to its own implicit searches.
- **Splicing stays in the typer.** The host only records quotes (tree, spliced type symbols,
  hole fillings); `inlining/splice.rs` builds the expansion by copying each quote with its
  holes filled and its spliced types replaced, then retypes it, which expands the inline calls
  in it (dotty's `typedExpr` after `expandMacro`).
- **Always on** in scripts, the REPL and the AOT compiler, which creates a VM over the library
  image for a compile; without a VM (typer reports) macro calls stay refused. Running library
  code at compile time is what dotty does too.

## Consequences
- A macro whose implementation is not on the VM's classpath, or that uses a `quotes.reflect`
  member outside the subset, a quote pattern other than a typed hole or `Expr.show`, is refused
  by name and the script falls back to scala-cli; an exception the implementation throws is
  dotty's "Failed to evaluate macro" error.
- Macro side effects happen in the VM that compiles the script. So that they do not reach
  the program, as they would not on the JVM, scivolo runs itself again in place of its process
  (`exec`, the hidden option `--rerun-of` giving the first start, from which `SCIVOLO_TIMEOUT`
  counts) after a compile that ran a macro, and the program runs from the cache in a fresh VM. Output a macro buffered in the VM without flushing is lost.
- The AOT compiler needs no rerun: macro state stays in its VM and cannot reach the native
  executable. It starts the VM for every compile missing the cache, so a VM that cannot start
  (e.g. a bad `SCIVOLO_NATIVE_LIBS`) fails sources without macro calls too.
- Scripts read the TASTy of `--cp` and `--dep` jars in the own frontend, as the REPL does
  (most of #128); the script cache key holds each classpath
  entry's path, size and time.
- The TASTy of every quote unpickled stays loaded in the universe (`Universe::load_quote`),
  since the symbols read from it point into it: a script pays per expansion, the REPL for the
  session.
- Definitions a macro's quotes or a library's inline body bring into the expansion take the
  call's position in the lowering, where dotty keeps their library offsets.
- Rejected: a second VM for macros (one Immix per process); Rust natives for all of `Quotes`
  (the hole closures would make natives re-enter the interpreter).
