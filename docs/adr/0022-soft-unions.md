# ADR 0022 — Soft and hard unions as a flag of the union type

Status: accepted · Date: 2026-10-12

## Context
Dotty distinguishes soft unions, formed as least upper bounds (branches of `if`, `match` and
`try`, several lower bounds of a type variable), from hard ones, written or read from TASTy
(`OrType.isSoft`). An inferred type widens a soft union to its join unless that is transparent
and keeps a hard one (`widenInferred`, `widenUnion`, `hardenUnions`). The typer kept the
softness per type variable and refused every inferred union.

## Decision
- **Representation:** `Type::Or(a, b, soft)`. TASTy `ORtype`, applied `|` and type trees make
  hard unions; `lub` makes soft ones unless asked otherwise (`lub_as`); type maps, `simplify`,
  `glb` distribution and balancing keep the flag of the union they rebuild.
- **Identity:** `Or(a, b, true)` and `Or(a, b, false)` are different interned types, as dotty's
  `eql` makes them; comparisons decompose either alike, so subtyping ignores the flag.
- **Type variables:** a variable is hard once a hard union is added as its lower bound; it is
  instantiated with `Widen::Singletons` (unions hardened), else with `Widen::Unions`.
- **Refusals:** a soft union whose join our `orDominator` cannot form (invariant arguments
  needing bounds) and joins intersecting a function class (dotty leaves some of their
  arguments unsimplified) stay unsupported.

## Consequences
- Every match on `Type::Or` names the flag; constructions choose it explicitly.
- The balanced-union memo is keyed by softness, and flattening stops at a union of the other
  softness, as dotty's `OrType.balanced` does.
- Kept unions reach the lowering, which erases them by `erasedLub`.
