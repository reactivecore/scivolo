# ADR 0090 — GADT bounds of first-order type parameters only

Status: accepted · Date: 2026-11-05

## Context
dotty narrows the bounds of type parameters by a case's pattern (`constrainPatternType` in
`GadtConstraintInference` mode) and keeps them in a `GadtConstraint` while the case is typed.
`TypeComparer` consults them only frozen, after the declared bounds (`gadtBounds`), and `adapt`
casts a tree that conforms only by them (`insertGadtCast`). The constraint also relates
parameters to each other, handles higher-kinded parameters (non-injective constructors),
alternatives (`necessaryGadtConstraint`) and refined singletons. We refused every pattern that
needed GADT reasoning (#41).

## Decision
- A case's pattern narrows the type parameters of the enclosing methods, and of the classes
  enclosing one, that occur in the selector type; the narrowing is computed by comparing the
  pattern type with the selector whose narrowable parameters are type variables of their
  bounds. A pattern variable made one with such a variable takes the first parameter's place.
- The bounds live beside the infos (`Universe::gadt_bounds`), restored when the case ends, and
  the comparer reads them frozen after the declared bounds.
- Out of the subset, refused as before: a selector with a refinement, a singleton, a
  higher-kinded parameter or an abstract type constructor applied; a pattern alternative that
  narrows; a typed pattern binding type variables (`case e: Pair[a, b]`) narrows nothing.
- Casts follow `insertGadtCast`: adapted trees, `if` branches, implicit arguments and
  arguments of implicit conversions.

## Consequences
- Parameters relate through bounds that name each other (`B >: A <: A`), not through an
  ordering; the GADT approximation of a parameter whose upper bound is `Any` takes a narrowed
  parameter bounded below by it.
- Member selection on a narrowed parameter (dotty's `tryGadt` healing) is not done: it stays
  refused as a missing member.
