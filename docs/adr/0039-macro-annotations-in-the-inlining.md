# ADR 0039 — Macro annotations expand in the inlining, on statement lists

Status: accepted · Date: 2026-10-24

## Context
A definition annotated with a class extending `scala.annotation.MacroAnnotation` is replaced by
the definitions `new annot(args).transform(using quotes)(definition, companion)` returns
(dotty's `MacroAnnotations`, run by its `Inlining` phase before the definition's inline calls
expand). Before this, the frontend ignored such annotations: `tests/run-macros`' `annot-*`
tests compiled silently wrong. A macro annotation's `transform` is an instance method of a
class compiled to NIR, unlike a splice's static macro implementation (ADR 0029).

## Decision
- **Where.** The inlining expands them on each statement list it visits (template, block,
  package) before it transforms the list's children, so the definitions returned are inlined
  like the rest and are not expanded again; the companion class definition in the same list is
  passed and replaced if returned. Definitions returned get their right-hand sides' local
  definitions owned by them and are entered among their class's declarations.
- **The instance.** The VM allocates the annotation's class, runs the constructor of the typed
  annotation `new annot(args)` with its literal arguments (refused otherwise), and calls the
  overlay's `QuoteHost.annotate(annotation, definition, companion)`, which runs `transform` and
  hands the definitions back through a `quotes.reflect` operation.
- **The trees.** Definitions returned keep their symbols: the copy that fills their quotes'
  holes maps their own and their nested definitions' symbols to themselves.
- **Order.** Annotations apply last first, as dotty keeps them on a symbol.
- **Refused.** An annotation on a definition the typer did not type (in a quote or a library
  inline body), a result dropping the annotated definition, definitions of another owner (a
  class added to a package from a package object) and non-literal arguments.

## Consequences
`run-macros`' annotation tests pass or are refused by name. The `transform` runs against the
same `Quotes` as splices, so every `quotes.reflect` member it needs is the subset's, with
`Symbol.spliceOwner` the annotated definition's owner.
