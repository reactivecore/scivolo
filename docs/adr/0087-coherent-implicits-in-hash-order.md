# ADR 0087 — `CanEqual` searches whose winner depends on dotty's hash order

Status: accepted · Date: 2026-11-03

## Context
dotty's search for a `CanEqual` instance is coherent: it takes the first candidate that
succeeds, without checking for ambiguity (`isCoherent` in `rank`). Candidates of one nesting
level and owner with as many implicit parameters tie in `compareEligibles`, so their order is
the order of `implicitMembers`, which iterates a `Set[Name]` keyed by the names' places in the
name table. `canEqualOption` and `canEqualOptions` are such a pair; which one dotty tries first
is not reproducible from the sources. mainargs compares `MainData.unapply` results with `==`,
whose nested `CanEqual` searches leave type variables open.

## Decision
- A search whose instance is dropped (`==` and pattern comparability checks, the reflexive
  probes of `strictEquality`) runs in `Mode::EXISTENCE`. There, a coherent search with open type
  variables takes the first candidate that succeeds, kept without a second attempt; the order
  cannot change whether one exists.
- Elsewhere a coherent search with several successful candidates is refused unless its type is
  fully defined and every candidate gives an argument of one type.

## Consequences
- `summon[CanEqual[Option[Int], Option[Nothing]]]` and similar explicit searches fall back.
- Nested searches under `==` cost one attempt per candidate and level instead of two, so deep
  tuples no longer exhaust the search budget.
