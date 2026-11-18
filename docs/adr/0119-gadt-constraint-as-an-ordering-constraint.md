# ADR 0119 — GADT constraint as an ordering constraint narrowed by the comparer

Status: accepted · Date: 2026-11-13

## Context
GADT bounds were narrowed by turning a pattern's narrowable parameters into fresh variables of
the ordinary constraint, comparing pattern and selector, and reading the bounds back, patched for
the orderings that constraint lacks: the alternative not taken. That constraint has no ordering
between variables and unifies `v <: w` at once, so the bounds, casts and binder infos dotty derives
from the order in which bounds arrive cannot be reproduced; the `tests/pos` files of issue #41 stay
refused.
dotty keeps GADT symbols as type references and narrows a separate `OrderingConstraint` from
inside the comparer (`narrowGADTBounds`), after ordinary type variables.

## Decision
- The GADT constraint is a store of its own, `GadtConstraint` (`crates/tasty/src/gadt/`): per
  registered symbol its bounds without the symbols it is ordered with, the symbols below and
  above it (transitively, most recently ordered first, as dotty's lists), an instance after
  `replace`, and the scope nesting level it was registered at. A change log gives `mark`,
  `rollback`, `changes_since` and `redo`, the shape of the ordinary constraint's.
- Keyed by symbol, not by internal type parameters: dotty internalizes a bound that is a GADT
  symbol at its top level, so `mapping`/`externalize` collapse to "a top-level reference to a
  registered symbol is that symbol". Difference case 2.
- `replace` substitutes the replaced symbol only where an entry is that symbol at its top level;
  dotty's dependency maps (`coDeps`/`contraDeps`), used to instantiate ordinary variables, have
  no counterpart. Difference case 2, except for one shape: dotty's `addToConstraint` also
  internalizes a registered symbol inside the unions of a declared upper bound and the
  intersections of a declared lower bound (`B <: A | Int`), and `replace` substitutes it there
  (`B <: String | Int` once `A = String`). The store keeps the symbol; a case that makes such a
  symbol equal to a type or unifies it is refused (`GadtConstraint::is_inexact`).
- The store lives on `Universe`, as the GADT bounds did, so every comparison sees it. It holds
  only what the cases being typed added and is empty between cases, so no file sees another's
  entries and the order of files cannot change a result. Typing files in parallel needs a store
  per thread: it then moves to the per-file comparison context (the `TypeVars` handle the
  comparer already receives); only `Cmp` and the typer's `Gadt` reach it.
- Registration follows dotty's order: per enclosing method from the outermost, the parameters
  of the classes between it and the next enclosing method, then those of a polymorphic function
  value it is the body of, then its own, at its nesting depth (`typedDefDef`); a pattern-bound
  type at `typedBind`; the binders of `maximizeType` all at once. It happens when a case first
  constrains a pattern, not at the definition: before that no comparison narrows and an
  unnarrowed entry gives no bound the declared one does not. Difference case 2.
- The mode is a flag of the store (dotty's `Mode.GadtConstraintInference`), so the checks the
  ordinary constraint runs while it is narrowed also narrow, as dotty's `addOneBound` in that
  mode does; `Universe::is_subtype`, used for `lub`/`glb`-like simplifications, never narrows.
- The typer reads `Universe::gadt_bounds`: the full bounds of a narrowed symbol (dotty's
  `fullBounds`, the ordered symbols joined in), the instance of one made equal to another type;
  the comparer reads the bounds of every registered symbol.
- A typed pattern that binds types and does not conform to the selector without GADT bounds
  registers each bound type one by one at the case's level (dotty's `newPatternBoundSymbol`),
  then constrains; `indexPattern` gives each its full bounds as info, an alias for one made
  equal to a type. A GADT approximation of a registered symbol is dotty's `approximation`: its
  instance, else its full upper bound (lower where contravariant or its upper bound refers to
  it).
- An extractor pattern whose argument type does not take the selector's is constrained in GADT
  mode and maximized as dotty's `maximizeType`, also for an open class with invariant
  parameters or an F-bounded one: the type variables it binds are replaced in the GADT
  constraint by their pattern-bound types, which are registered all at once at the case's level.
- While the GADT constraint is inferred, a type variable's new upper bound is met with its old
  one comparing with the type variables' bounds (dotty's `hi & bound`), so that a GADT symbol
  bounded by a type with a variable drops a part it already is below.
- A pattern-bound type registered in the GADT constraint has known bounds (its full bounds as
  info), so a case's type that refers to it is avoided through them. It does not keep a match
  type from being reduced, as dotty's reduction footprint holds only type parameters; a class's
  higher-kinded type parameter does while a case is typed, as dotty registers it at every
  method and ours only once a case constrains.
- Higher-kinded type parameters are registered as the others. An application of one is compared
  as dotty's `compareAppliedType1/2`: constructors equal by their GADT bounds or ordering, the
  arguments compared with the GADT constraint frozen unless one constructor is a class and the
  bound used made the other equal to it (`tyconIsInjective`); else through the GADT bounds of
  the constructor, frozen (`byGadtBounds`). An
  extractor pattern outside a case that would need GADT mode is refused (pattern definitions
  are cases after desugaring).
- While the GADT constraint is inferred, a type variable whose bounds become the same type
  constructor is instantiated to it, as dotty's `tryReplace`; outside, it stays a variable.
- `compareAppliedType2` compares with a tuple class applied by its nested pairs first, ending in
  `EmptyTuple.type`, then by base type, as dotty: the order decides which bounds a GADT
  comparison narrows.

- The comparer narrows the store only in GADT mode (`Universe::constrain_gadt_in`, necessary
  constraints only): `third_try_named` adds a lower bound or an ordering to the right-hand
  symbol, `fourth_try` an upper bound to the left-hand one, through `addBound`, `addLess`,
  `unify`, `addBoundTransitively` and `addOneBound` as dotty's `GadtState`. A failed bound
  rolls the store back; `necessaryEither` keeps the GADT state of the subsumed branch or none,
  `sufficientEither` leaves it alone, as dotty.
- A bound reached through another abstract type's bounds or a singleton's underlying type
  narrows nothing (`ApproxState`); the state is reset where dotty starts a fresh `isSubType`:
  type arguments, prefixes and the members of refinements.
- `unify` gives the kept symbol both bounds without dotty's `validBoundsFor` and
  `LevelAvoidMap`: with `Config.checkLevelsOnConstraints` off the map avoids nothing and GADT
  bounds hold no wildcards; `validBoundsFor` changes only internal references below the top
  level, which the refused shape above alone has. Difference case 2.

## Consequences
- The GADT shapes still refused are listed in `docs/conformance.md`; each is dropped only with a
  golden typed test of dotty's tree.
