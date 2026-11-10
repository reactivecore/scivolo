# ADR 0113 — Components own their state

Status: accepted · Date: 2026-11-09

## Context
The typer's subsystems are written as `impl Typer` blocks: 236 methods in `typer/implicits/` alone,
with the state of one implicit search (its budget, the candidates it leaves out) as fields of
`Typer`. Any typer method can reach that state and any helper of the subsystem, and wrapping the
same methods in a view struct over `&mut Typer` only moves them, not the state. dotty runs an
implicit search as an object, `ImplicitSearch(pt, argument, span)(using Context)`, created,
run and dropped.

## Decision
- A component is a struct created for one bounded computation, run, then dropped.
- It owns the data of that computation as fields; that state does not live on `Typer`.
- Shared services come in as arguments, as dotty's `Context`: the `Universe`, the constraint, and
  the typer for typing a candidate, `conforms` or a nested search. Its methods take them, e.g.
  `fn run(self, t: &mut Typer) -> …`; no `Deref`, no trait or generic for the shape.
- The subsystem's helpers leave `Typer`: onto the component when they use its state, else free
  functions taking only what they use (`&mut Universe`, `&Constraint`, or the typer).
- `Typer` keeps only the entry points the rest of the typer calls, under their names.
- A nested computation creates a new component.

One component, one module directory: `<component>/mod.rs` holds the struct, its entry and the
component's doc comment, which says what it does and which Scala code it handles. Its other files
are private submodules, each exposing one entry to `mod.rs`, or as few as the call graph forces;
everything else in them is private, and they may each add an `impl` block of the component.
Service types (`TypeOps`) and scoped state structs are not components and keep their files. An
item used from outside the component is re-exported from `mod.rs` only when the call graph forces
it; such a shared helper rather moves out, to `TypeOps`, a `Symbol` method or a neutral module.

## When
- For bounded computations and tree passes with their own state: an implicit search
  (`ImplicitSearch`), an implicit scope (`ImplicitScope`, `Parts`), a divergence measure.
- Not for the core `typed` recursion, whose context is the typer itself.

## Consequences
- The core implicit search (`search`, `attempts`, `history`, `divergence`, `measures`, `ranking`,
  `contextual`, `candidates`, `scope`, `parts`, and their helpers) follows this, and so does
  synthesis (`Synthesizer`, `MirrorSynthesis`); views and the application of implicit clauses
  are still `impl Typer`.
- State shared across nested searches that run through the typer (the open searches, the by-name
  dictionary, the budget) stays in `Typer::history`, as dotty keeps it in the `Context`.
- Type relations and queries that need only the universe and the constraint (`conforms`, `zonk`,
  `open_vars`, `wild_approx`, …) are a service type, `TypeOps` from `Typer::types()` (dotty's
  `TypeComparer`), for completed symbols; helpers that need no more take it, and `Typer`'s methods
  of the same names forward to it. Flag and kind checks are methods on `tasty::Symbol`
  (`is_implicit`, `is_given`). Helpers that complete symbols, type trees or search take the typer.
