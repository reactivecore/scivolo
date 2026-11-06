# ADR 0092 — GADT: higher-kinded parameters, typed patterns, upcasts, member healing

Status: accepted · Date: 2026-11-06 · Amends: ADR 0090

## Context
ADR 0090 narrowed first-order type parameters only and refused higher-kinded parameters,
typed patterns binding type variables, inline matches and member selection on narrowed types.
dotty's `GadtConstraint` handles all of them.

## Decision
- Higher-kinded type parameters narrow like first-order ones; a selector argument may be a
  type lambda. The comparer reads an abstract type constructor's GADT bounds frozen after its
  declared ones (`F[X] <: T` through its upper, `T <: F[X]` through its lower bound applied).
- A typed pattern binding types (`case e: Pair[a, b]`) whose type does not conform to the
  selector compares with the bound types as type variables; the selector's parameters get
  their GADT bounds as in ADR 0090, the bound types the bounds the comparison leaves them.
- `this` as selector stands for its class applied to its type parameters, seen from `this`;
  a singleton pattern refines invariantly (dotty's `refinementIsInvariant`).
- A pattern type of a class unrelated to the selector's is compared with the selector's base
  types at the shared parents (`constrainUpcasted`); a union pattern type that does not
  conform is compared by operand like a union selector.
- The type parameters of a polymorphic function's `apply` narrow in its body; the cases of an
  inline match narrow like others.
- A selection whose member is missing retries on the qualifier cast to its GADT approximation
  (`tryGadt`), after extensions and before implicit conversions.

## Consequences
- Bounds between parameters are not propagated (`X <: A <: B <: Int` gives `X` no bound), and a
  parameter narrowed twice keeps the later bounds: those patterns stay refused.
- A pattern-bound type takes the variable's instance, not dotty's full bounds, so a value of a
  type equal to it only by GADT bounds is cast where dotty needs no cast.
