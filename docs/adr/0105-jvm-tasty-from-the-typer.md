# ADR 0105 — The JVM backend's TASTy from the typer's trees before inlining

Status: accepted · Date: 2026-11-08

## Context
A class file dotty writes carries a `TASTY` attribute with the UUID of its `.tasty` file, which
dotty's Pickler phase writes after PostTyper and before Inlining. Scala 3 compilers read a
Scala class only through that TASTy. Our typer expands inline calls in place, in `Units::finish`,
and the lowering then changes symbols (owners, names, flags), so the trees dotty pickles exist
only inside the typer, between the synthetic members and inlining.

## Decision
- `Units::tasty_sources` asks the typer to pickle; `compile_units` sets it for `Backend::Jvm`
  only. The typer pickles each top-level class (`typer::pickle::pickle_file`) after the
  synthetic members and before inlining, with the quote pickler extended to files.
- A tree the pickler lacks refuses the whole unit (`pickling … into TASTy`): no class file is
  written without its TASTy.
- `generate` gets the pickles; the generator puts the `TASTY` attribute on the class dotty puts
  it on (the mirror class of a lone top-level object, else the class) and writes the `.tasty`
  beside it.

## Consequences
- The NIR path pickles nothing and is unchanged.
- The TASTy is semantically dotty's, not byte-identical: where our types tell apart what
  dotty's identity-shared types do not (or the reverse), the `SHAREDtype` choices differ.
