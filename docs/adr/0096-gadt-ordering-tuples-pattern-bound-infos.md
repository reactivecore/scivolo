# ADR 0096 — GADT: related parameters, tuple pairs, pattern-bound infos

Status: accepted · Date: 2026-11-08 · Amends: ADR 0090, ADR 0092

## Context
ADR 0092 left these GADT shapes refused: bounds between parameters not propagated, a tuple
class in a GADT approximation, a pattern-bound type unnarrowed by nested patterns, a variable
pattern of a higher-kinded parameter, the type parameters of a polymorphic function value.
dotty's `GadtConstraint` keeps every narrowable symbol in one ordering constraint (`addLess`),
records `T <: TupleN[..]` as `toNestedPairs`, and gives the binds of a case's pattern their
GADT bounds as infos (`indexPattern`).

## Decision
- A pattern's narrowing compares with type variables for the selector's narrowable parameters,
  those of the pattern type and of its base type at the selector's class, and those their
  bounds name, transitively; the pattern compares by that base type.
- A narrowed parameter whose new bound names another one also bounds that one, on the other
  side; top operands of an intersection upper bound are dropped. A pattern-bound type of a
  typed pattern gets those orderings too.
- A GADT approximation follows parameters made one to the last of them.
- A new GADT upper bound applying `scala.TupleN` is held as `*:` pairs ending in
  `EmptyTuple.type`; a tuple class in an approximation is no longer refused.
- The comparer reads an abstract type constructor's applied GADT bounds with the type
  variables free (dotty's `inFrozenGadt`), not frozen.
- After a case's pattern, its pattern-bound types take their GADT bounds as infos.
- A variable pattern for a higher- or any-kinded class parameter takes that parameter's
  bounds (dotty's `tparamBounds` as the expected type of `patternVar`).
- The type parameters of a polymorphic function value narrow inside its closure.
- An extractor pattern that does not conform to a selector whose only abstract arguments are
  opaque types is typed without narrowing, as no GADT bounds narrow an opaque type.
- A block adapted to a type it conforms to only by GADT bounds casts its expression, a
  closure block as a whole.
- The pattern matcher binds an extractor's component of a pattern-bound type as `Any`
  (`avoidPatBoundType`).

## Consequences
- Refused still: a singleton cast by GADT bounds as an inline argument (dotty types the
  expansion by the singleton); a GADT approximation inside an inferred type argument
  (`i4075-gadt`); a typed pattern's bound type made one with several parameters (dotty
  bounds it by each compared type).
