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
  synthesis (`Synthesizer`, `MirrorSynthesis`), the application of an implicit clause
  (`ImplicitClause`), the dictionary of by-name implicits (`Dictionary`, which takes them out
  of `Typer::history`), the search for a view (`ViewSearch`: conversions, extension
  containers, the view on a qualifier), the override checks of a class (`OverrideChecks`),
  kept in `Typer::macro_annotated` until macro annotations expanded, the `derives` clause of a
  class (`Deriver`; its instances wait in `Typer::derivations` until typed), a derived type tree
  (`DerivedTypeTree`), making an inline method inlineable (`MakeInlineable`; its accessors
  wait in `Typer::accessors` until their class's body is typed), an extractor pattern
  (`UnapplyPattern`) and the expected types of its patterns (`UnapplyArgs`).
- State shared across nested searches that run through the typer (the open searches, the by-name
  dictionary, the budget) stays in `Typer::history`, as dotty keeps it in the `Context`.
- Type relations and queries that need only the universe and the constraint (`conforms`, `zonk`,
  `open_vars`, `wild_approx`, …) are a service type, `TypeOps` from `Typer::types()` (dotty's
  `TypeComparer`), for completed symbols; helpers that need no more take it. So do the inference
  queries that need no completion (variances, widening, `seen_from`, simplification, escaping
  references). `Typer` keeps methods of the same names that forward to it only where many
  callers use them; the others call `t.types()`. Flag and kind checks are methods on
  `tasty::Symbol` (`is_implicit`, `is_given`). Helpers that complete symbols, type trees or
  search take the typer.
- The checks after typing are components that collect their errors for `Typer` to report:
  `VarianceCheck` (over the `VarianceOps` service), `ForwardRefs`; so are the top-level
  annotations (`TopLevel`) and the `-Wnonunit-statement` lint (`Linter`). The variances inferred
  for aliases and the variables assigned non-locally stay on `Typer`: typing fills them.
- Avoiding local symbols in a type is a component, `Avoid`, whose methods take the typer: it
  completes the members of approximated prefixes and instantiates type variables.
- The narrowing of the GADT bounds by one pattern is a component, `Narrowing`, whose methods
  take the typer: comparing types completes aliases. The GADT bounds themselves are scoped
  state (ADR 0114).
- Overload resolution is a component, `Resolution` (the prefix and the expected type), whose
  methods take the typer: it types arguments while comparing alternatives. It returns the
  ambiguous alternatives with its error. The prototypes (`protos`, `poly_protos`), the copies of
  alternatives (`mapped_alts`) and the type arguments of the resolution of `f[Int]` in progress
  (`mapped_targs`, read while completing and searching implicits inside it) stay on `Typer`.
- Lifting the parts of one application into local values is a component, `Lifter`, whose
  methods take the typer: deciding purity completes methods. Purity has no state of its own; its
  helpers are free functions taking the typer.
