# ADR 0088 — Implicit candidates interpolated only when applied, completed before their attempt

Status: accepted · Date: 2026-11-04 · Amends ADR 0026

## Context
dotty's `typedImplicit` adapts a candidate reference to the expected type. `adapt` does not
interpolate: only `typed`'s `simplify` and `readaptSimplified` do. A candidate with implicit
parameters is interpolated once its arguments are applied (`addImplicitArgs`); a candidate
without them keeps its type variables, which the enclosing application's interpolation
instantiates later (`util.Random.shuffle(xs)`: `BuildFrom`'s `CC` from the lower bound `xs`
gives it). We refused such a candidate when its value type applied an open type constructor
variable (#123). dotty completes a definition in its creation context, outside the
`TyperState` of the candidate that first needs it (`completeInCreationContext`); we completed
inside the candidate's attempt (#103).

## Decision
- A candidate is interpolated against the expected type after its implicit arguments are
  applied; a candidate without implicit parameters is not, and its open variables, constructor
  variables included, stay for the enclosing typing.
- `fitting`, `best_first` and `first_fitting` complete a candidate's definition before its
  attempt starts; `Constraint::keep` still guards completions inside nested attempts.

## Consequences
- ADR 0026's remark that our search does not interpolate a candidate's application holds only
  for candidates without implicit parameters, as in dotty.
- A candidate whose completion records a refusal refuses the unit directly, not as a candidate
  outside the subset; both fall back.
