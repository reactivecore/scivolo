# ADR 0027 — Opaque types: the comparer knows the owner, the lowering elides aliases

Status: accepted · Date: 2026-10-15

## Context
dotty gives an opaque type bounds as its info and keeps the alias as a refinement of its
owner's self type, so only references through `M.this` see it. Inside `M`, a value typed
outside (`M.x: M.T`) would not see it either; `TypeComparer.liftToThis` retries a failed
comparison with references to an enclosing opaque object `M` made `M.this`, deciding
"enclosing" by `ctx.owner`. Our comparer (`tasty::subtype::Cmp`) has no context, and the typer
calls it from many places. After `RefChecks`, `ElimOpaque` turns opaque types into aliases for
every later phase. Our universe is shared across REPL inputs, which must keep the types opaque.

## Decision
- **Visibility:** the universe keeps the aliases of script opaque types apart from their infos
  (bounds) and returns one only for a reference whose prefix is its owner's `this`, and, while
  the typer sets an owner (below), only inside that class. `as_seen_from` keeps no `this`
  prefix of an object with opaque types as static (dotty's `seesOpaques`).
- **Owner on the comparer:** the universe carries the owner the comparisons are made in; the
  typer sets it on entering `typed` and a completion, and restores it on leaving. `Cmp` lifts with it where dotty
  calls `tryLiftedToThis1`/`2` for abstract references; lifting in the typer instead was
  rejected, as it would have to anticipate every comparison.
- **Elision:** the lowering's `elim_opaque` (before `explicit_outer`) makes every prefix see the
  aliases and the end of the lowering undoes it, so the next REPL input sees opaque types again.
  `==`/`!=` on opaque values, which dotty resolves again, are refused.

## Consequences
- Outside its scope an opaque type is abstract; a missing lift shows as a refusal or an
  error, never as an alias seen where dotty would not see it.
- Comparisons made outside the typer (the lowering) lift nothing; there the owner is unset,
  which counts as inside every object, so only the prefix decides visibility.
- Refused for now: higher-kinded and top-level opaque types, opaque types outside objects,
  aliases other than a class, inline methods inside an object with opaque types, and a
  conformance that holds only through a lift (where dotty inserts a `Typed`). dotty's
  member-selection healing (`Typer.typedSelect`'s `liftToThis`) is not ported.
