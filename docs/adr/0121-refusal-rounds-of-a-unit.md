# ADR 0121 — Refusal rounds of a unit: each stage reports every refused file, files are retyped

Status: accepted · Date: 2026-11-15

## Context
Under `--leave-out-refused` (ADR 0116) a unit of several files is typed again after each round
of refusals, the refused files blanked. A round costs a whole typing: scalalib's 600 files take
five, the compiler's (#241) more. dotty's `Run.compileUnits` runs each phase over all units and
reports the errors of every unit before it stops; it never retypes. Leaving the refused files
out after typing, without retyping (!818), lost the super accessors and mixin members that the
lowering of a refused file adds for the classes of other files.

## Decision
- Each stage reports every file refused in it before the unit is retyped, as dotty reports
  the errors of all units of a phase:
  - typing: the first unsupported construct of each file (`Typer::unsupported`), and the type
    errors of all files, also when another file has an unsupported construct
    (`Units::errors`);
  - the stages after typing (annotations, variances, synthetic members, pickling, inline
    calls, forward references) run on the files without refusals or type errors, so that
    their refusals are found in the same round;
  - the lowering stops at its first refusal: its phases walk the whole unit and share state
    across files (super accessors, bridges, lifted classes), so a phase cannot drop one file
    and go on.
- The refused files are blanked and the unit typed again in a fresh universe: the files
  that need a refused one see it missing, as a build without it would, and the lowering
  runs over the trees of exactly the accepted files.
- `Isolated::typings` counts the typings; the binaries print it under `--leave-out-refused`
  and `bench-compile` reports it.

## Consequences
- A unit takes one typing more than the depth of its chains of refusals: a file refused only
  because a file it needs was left out is found one round after that file. scalalib's rounds
  are all such chains; they shrink as refusals are fixed, not by this ADR.
- A file with type errors is refused in the round its errors appear, with the unsupported
  constructs of the others: a file typed after a refused one may be refused for its error
  trees and left out with it (as in ADR 0116).
- Each lowering refusal costs a round.
