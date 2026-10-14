# ADR 0023 — Macros with intrinsic expansions until phase 4

Status: accepted · Date: 2026-10-14

## Context
Scripts call library macros the own frontend cannot run before phase 4 (macros in the VM). Some
of them dotc never runs as macros either: `StringContext.s`, `raw` and `f` are Scala 2 macros
(`macro ???` in the 2.13 library) that the typer lets through (`Typer.adaptNoArgsOther`) and the
post-typer phase `StringInterpolatorOpt` expands as compiler intrinsics, `f` after the format
check of `TypedFormatChecker` (`transform/localopt/`). A format the check rejects is a compile
error; an argument it can only pass through an implicit view gets the view from a typer search
at that phase.

## Decision
- A macro gets an expansion in the own frontend only where dotc expands it as an intrinsic, and
  in the phase and form dotc uses; ADR 0010's byte equality is the check. Today: `s`, `raw` and
  `f` in the lowering's `interpolators` phase.
- What dotc reports as an error, and what needs an implicit search after the typer (`f`'s
  views), is refused, so the script falls back to scala-cli, which reports the error.
- Every other macro (Scala 3 splices, Scala 2 macros dotc rejects) stays refused ("inline call
  not expanded") until phase 4.

## Consequences
- `f` interpolations with argument types the conversions take lower without a JVM;
  `f"$i%f"` of an `Int` falls back.
- The format check's warnings are not reported.
- The `interpolators` phase walks children first, as dotty's `MegaPhase` does, so an
  interpolation nested in an `s`, `raw` or `f` one gets dotty's spans; one nested in a
  user-defined interpolator may not.
- Rejected: expanding `f` in the typer's inlining phase (dotc does not, so trees and spans would
  differ from the oracle's).
