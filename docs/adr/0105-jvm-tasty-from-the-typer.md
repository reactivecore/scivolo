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
- The pickler adds what dotty's trees have by then and ours get later or never: the super
  accessors of calls `C.super[M].m` (`SuperAccessors`: in a trait without `M`, from a class nested
  in `C` or a value class's method; the generator refuses a unit whose lowering made other
  accessors, such as those of protected members), named type arguments by position
  (`PostTyper.normalizeTypeArgs`) and the `@Child` annotations of sealed classes
  (`Namer.addChild`).
- `generate` gets the pickles; the generator puts the `TASTY` attribute on the class dotty puts
  it on (the mirror class of a lone top-level object, else the class) and writes the `.tasty`
  beside it.

## Consequences
- The NIR path pickles nothing and is unchanged.
- The TASTy is not byte-identical, an open difference: dotty shares a type by identity
  (`pickledTypes`, an `EqHashMap`), we by its bytes, so the `SHAREDtype` choices differ where our
  synthetic trees build an equal type anew or dotty's are distinct instances.
- Also open, not accepted: positions missing for the super accessors' definitions and the
  annotation in an annotated type (`T @uncheckedVariance` of a default getter).
- Refused still: the bounds of a type lambda whose declared variances the universe does not know
  (one read from a library's TASTy), a reference to a method a refinement declares, super calls of
  methods with a target name, implicit parameters or type parameters after a parameter list.
