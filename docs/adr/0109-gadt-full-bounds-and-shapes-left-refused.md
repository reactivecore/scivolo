# ADR 0109 — GADT: full bounds through pattern-bound types, shapes left refused

Status: accepted · Date: 2026-11-09 · Amends: ADR 0096

## Context
ADR 0096 left fifteen `tests/pos` GADT rows refused or differing. In dotty a pattern-bound
type made for an extractor's type argument (`maximizeType`) takes the argument's full bounds,
parameters below it included (`fullLowerBound`), and a GADT approximation drops such a type from
a parameter's upper bound when the parameter is its lower bound, keeping the bound the
constraint propagated to the parameter (`fullUpperBound`). Our constraint keeps an ordering
between variables on one side only, so the pattern-bound type had no lower bound and the
approximation kept it: `gadt-cast-singleton` cast `x` to `x & S$1` where dotty casts to `x & L`.

## Decision
- A pattern-bound type made for an extractor's type variable takes as lower bound, besides the
  variable's own, each GADT variable whose upper bound is that variable or an intersection
  with it.
- A GADT approximation's upper bound replaces a pattern-bound type whose lower bound the
  parameter conforms to by that type's upper bound, the parts of an intersection joined by
  `glb`.
- An application of a value without `apply` retries on the value cast to its GADT
  approximation (`tryGadt` in the `typedSelect` of `tryInsertApplyOrImplicit`).

## Consequences
Refused still, each with what it lacks:
- `i4075-gadt`, `gadt-TypeSafeLambda`: an inferred type argument or an adaptation that dotty's
  constraint solves through a GADT-bound pattern type (no propagation of bounds between
  pattern-bound types and parameters beyond the shape above).
- `i4471-gadt`, `injectivity-gadt`: a typed pattern's bound type made one with parameters
  that are made one with each other; dotty bounds it by each compared type.
- `i19570.min2`, `i19570.orig`: a typed pattern's bound type made one with a polymorphic
  function value's parameter; dotty keeps the bound type as the representative.
- `i13974a`, `i19570.min1`: an inline argument cast by GADT bounds that is a singleton or that
  the call's type refers to by a skolem.
- `i16785`, `i16997`: a class's higher-kinded type parameter in a pattern's GADT comparison.
- `inline-match-gadt`: a singleton's GADT cast in an inline match case, which dotty casts twice.
- `i15867`: an implicit conversion of a GADT-approximated qualifier; dotty applies it to the
  unhealed qualifier with the argument cast, inferring the conversion's type argument from it.
- `gadt-alt-doc1`: an F-bounded type variable instantiated to a type containing it.
- `gadt-infer-ascription`: now healed, then refused at `Tuple.apply`'s match type `Elem`
  (case type with pattern variables).
