# ADR 0015 — Atoms of types, cached, with unmodelled shapes falling through

Status: accepted · Date: 2026-10-08

## Context
Dotty decides comparisons and lubs of unions of singletons by their atoms (`Type.atoms`,
`compareAtoms`, the atoms step of `mergedLub`), caches the atoms of each union (`myAtoms`,
not while provisional), and rebuilds a union balanced whenever its sides' sizes differ by more
than one (`OrType.balanced`). A list literal of N constants grows the lower bound of its
element type one union at a time, so every step rebalances; our types are hash-consed and
never freed.

## Decision
- **Three outcomes:** atoms are known, unknown as in dotty, or unmodelled: the shapes whose
  atoms dotty may know and we do not compute (binder-bound parameters, `RecThis`, unresolved
  names, a match type whose bound has no known atoms, a part that may or may not be
  `Nothing`). Only dotty's unknown gives `a <: Nothing`; unmodelled gives no answer, so the
  comparison continues by the parts.
- **Cache:** the atoms of a union are cached by its `TypeId` when computing them read no type
  variable, no symbol and did not hit the depth limit. Atom and part lists are shared: a list
  only grows at its end, and a union extending another extends its list in place.
- **Shapes:** unions keep dotty's balanced shape exactly. The balanced union of each range of
  a part list is memoised, so a rebuild only interns the subtrees it has not built before.

## Consequences
- An incremental union of N parts interns about N^1.5 distinct union nodes (4.7 million for
  N = 32000), all kept; time grows with them, memory too (1.2 GB for N = 32000).
- Unions with singletons that are paths are not cached; their atoms are recomputed on use.
