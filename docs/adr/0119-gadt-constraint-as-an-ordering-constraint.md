# ADR 0119 — GADT constraint as an ordering constraint narrowed by the comparer

Status: accepted · Date: 2026-11-13 · Amends: ADR 0090, 0092, 0096, 0109

## Context
ADRs 0090–0109 narrow GADT bounds by turning a pattern's narrowable parameters into fresh
variables of the ordinary constraint, comparing pattern and selector, and reading the bounds
back, patched for the orderings that constraint lacks. That constraint has no ordering between
variables and unifies `v <: w` at once, so the bounds, casts and binder infos dotty derives from
the order in which bounds arrive cannot be reproduced; the `tests/pos` files of issue #41 stay refused.
dotty keeps GADT symbols as type references and narrows a separate `OrderingConstraint` from
inside the comparer (`narrowGADTBounds`), after ordinary type variables.

## Decision
- The GADT constraint is a store of its own, `GadtConstraint` (`crates/tasty/src/gadt/`): per
  registered symbol its bounds without the symbols it is ordered with, the symbols below and
  above it (transitively, most recently ordered first, as dotty's lists), an instance after
  `replace`, and the scope nesting level it was registered at. A change log gives `mark`,
  `rollback`, `changes_since` and `redo`, the shape of the ordinary constraint's.
- Keyed by symbol, not by internal type parameters: dotty internalizes only a bound that is a
  GADT symbol at its top level, so `mapping`/`externalize` collapse to "a top-level reference to
  a registered symbol is that symbol". Difference case 2.
- `replace` substitutes the replaced symbol only where an entry is that symbol at its top level;
  dotty's dependency maps (`coDeps`/`contraDeps`), used to instantiate ordinary variables, have
  no counterpart. Difference case 2.
- The store lives on `Universe`, as the GADT bounds did, so every comparison sees it. It holds
  only what the cases being typed added and is empty between cases, so no file sees another's
  entries and the order of files cannot change a result. Typing files in parallel needs a store
  per thread: it then moves to the per-file comparison context (the `TypeVars` handle the
  comparer already receives); only `Cmp` and the typer's `Gadt` reach it.
- Registration follows dotty: the enclosing classes' parameters and then the method's at
  `typedDefDef` (the closure of a polymorphic function value likewise), a pattern-bound type at
  `typedBind`, the binders of `maximizeType` all at once.

- The comparer narrows the store only in GADT mode (`Universe::constrain_gadt_in`, necessary
  constraints only): `third_try_named` adds a lower bound or an ordering to the right-hand
  symbol, `fourth_try` an upper bound to the left-hand one, through `addBound`, `addLess`,
  `unify`, `addBoundTransitively` and `addOneBound` as dotty's `GadtState`. A failed bound
  rolls the store back; `necessaryEither` keeps the GADT state of the subsumed branch or none,
  `sufficientEither` leaves it alone, as dotty.
- A bound reached through another abstract type's bounds narrows nothing (`ApproxState`); the
  state is reset per type argument, where dotty starts a fresh `isSubType`.
- `unify` gives the kept symbol both bounds without dotty's `validBoundsFor` and
  `LevelAvoidMap`: GADT bounds hold no type variables of deeper levels and, the symbol being
  keyed, no internal references to it. Difference case 2.

## Consequences
- The read-back rules of ADRs 0090, 0092, 0096 and 0109 go as the comparer narrows the store;
  each refusal they list is dropped only with a golden typed test of dotty's tree.
