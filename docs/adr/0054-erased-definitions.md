# ADR 0054 — Erased definitions: typed as dotty types them, dropped by the lowering

Status: accepted · Date: 2026-10-26

## Context
`erased` (`experimental.erasedDefinitions`) marks parameters and values that exist only for
the type checker; values of a class deriving from `compiletime.Erased` (`CanThrow`) are erased
too. dotty records an erased parameter as an `@ErasedParam` annotation on its type in the
method type, gives a function type or closure with an erased parameter the type
`PolyFunction { def apply(erased x: A): R }`, and drops erased definitions, parameters and
arguments in `Erasure`, checking that the arguments are pure and that nothing else refers to
an erased value. The typer refused all of it.

## Decision
- The typer and the TASTy decoder build method types as dotty does: `@ErasedParam` on the
  types of erased parameters, also those erased by their class; signatures and the mangled
  erasure skip them, and `PolyFunction` refinements erase to the function class of their
  arity without them.
- Function types and closures with erased parameters, also contextual ones, are
  `PolyFunction` refinements; `throws` types and the `CanThrow` givens of a `try` follow
  `saferExceptions` in the typer.
- The lowering drops erased values, parameters and arguments in one pass before local classes
  and lambda lifting (`erasure/erased_defs.rs`), refuses an argument that is not pure and any
  reference to an erased value left; the phases before it see the full method types.

## Consequences
- 17 of research 38's 21 tests of erased definitions lower as Scala Native compiles them.
- Purity of erased arguments and values follows the inliner's `isElideableExpr` with
  `isPureExpr`'s type applications, plus the erased values of `scala.caps` and references to
  inline values: never pure where dotty finds it impure, so it refuses some arguments dotty
  accepts.
