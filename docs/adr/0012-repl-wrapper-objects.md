# ADR 0012 — REPL inputs as wrapper objects

Status: accepted · Date: 2026-10-07

## Context
A REPL input must see the definitions of earlier inputs and run without recompiling them. The
own frontend types one unit at a time into a `tasty::Universe`; the VM loads NIR lazily by
top-level name, and the collector serves one VM per process.

## Decision
- One `Universe` and one `Vm` per session. Each input becomes one unit:
  `package <empty> { <imports>; object rs$line$N { <stats>; <show methods> } }`, as dotty's
  `ReplPhase` wraps it; its NIR is added to the VM before anything looks it up.
- Earlier inputs are seen through import statements at the start of the package, not through
  typer contexts: `import rs$line$i.{x as _, given, *}` and that input's own top-level imports,
  kept as source text and parsed again. The `as _` selectors hide the names a later input defines
  again as a term or as a type, since imports in one package body share a scope and would be
  ambiguous. An import path that starts at a definition of an input is kept qualified by its
  wrapper (`import rs$line$1.s.length`), so that hiding `s` later does not break it.
- Values are shown by a synthesized show method per value, typed with the input, which the VM
  calls after constructing the module; how they print is ADR 0014.
- Values of an inferred type `Unit` stay fields of the wrapper and are not printed.
- The wrapper index `N` always advances, also after a failed input, and across `:reset`, so
  names never clash in the VM; the `res` index advances once an input parses, also when its
  typing fails, and the `res` values of type `Unit` give their indices back once the module
  was constructed, as in dotty's `renderDefinitions`. Only inputs whose module was constructed
  are imported later. Output shows `rs$line$N` less the index at the last `:reset`, so the
  user's numbers restart at 1 as in dotty.
- Diagnostics print as dotty's `MessageRendering` draws them without colors, positioned in the
  input's own text: syntax errors alone, the diagnostics of an input that fails to compile then
  their count (`1 error found`), and the warnings of one that compiles after their count.
- `:type` wraps its input like other inputs, but in object `rs$line$type$K`, a fresh name, and
  only types it: it takes no wrapper or `res` index and is never imported. It prints the
  widened type of the last statement, of the expression for one that became a `res` value.

## Consequences
- The typer and the lowering need no REPL mode.
- A module whose constructor threw stays cached in the VM.
- The universe keeps the symbols of `:type` wrappers.
- `x as _` hides both kinds: an input defining a type and a term `x` loses the type once a later
  input defines the term `x` alone, where dotty keeps it.
- A later definition of a name an earlier user import brings is ambiguous, as in dotty, but
  for a wildcard import, where dotty's nested contexts pick the definition.
- Traces show the interpreter's frames; `... N elided` and `... N more` count those, not the
  JVM's reflection frames. A trace holds at most 64 throwables. A throwable met before is left
  out also when it was only met as a wrapper dotty's `rootCause` unwraps.
