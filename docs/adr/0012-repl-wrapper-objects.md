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
- Values are shown by a synthesized `def x$show: String = ScalaRunTime.stringOf(x, 1000)` per
  value, typed with the input; the VM calls it after constructing the module. Strings, characters,
  `Long` and `Float` are quoted and suffixed by the erasure of the static type, where dotty's
  pprint goes by the runtime class (`val a: Any = "a"` prints unquoted).
- Values of an inferred type `Unit` stay fields of the wrapper and are not printed.
- The wrapper index `N` always advances, also after a failed input, and across `:reset`, so
  names never clash in the VM; the `res` index advances once an input parses, also when its
  typing fails, and the `res` values of type `Unit` give their indices back once the module
  was constructed, as in dotty's `renderDefinitions`. Only inputs whose module was constructed
  are imported later.

## Consequences
- The typer and the lowering need no REPL mode.
- A module whose constructor threw stays cached in the VM.
- `x as _` hides both kinds: an input defining a type and a term `x` loses the type once a later
  input defines the term `x` alone, where dotty keeps it.
- A later definition of a name an earlier user import brings is ambiguous, as in dotty, but
  for a wildcard import, where dotty's nested contexts pick the definition.
- Values print as `stringOf` does, not as pprint: products and collections of strings differ.
- Traces show the interpreter's frames; `... N elided` counts the wrapper frames cut, not the
  JVM's reflection frames.
