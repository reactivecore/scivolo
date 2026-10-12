# ADR 0020 — The `inlining` phase runs at the end of the typer, the typer its host

Status: accepted · Date: 2026-10-12

## Context
`summonInline`, `summonAll` and `summonFrom` search implicits where the outermost inline call
was written, after typing: dotty's `Inlining` phase is a `TreeMapWithImplicits`, which rebuilds
the call's context from the tree, and the `InlineTyper` enters an inline body's own givens as it
retypes it (`N/Inlines.scala:530`, `N/InlineReducer.scala:212`, `N/Inliner.scala:921`). Our
implicit search is a method of `Typer`, over its levels and imports; `Inlining` borrows the
typer's universe, trees and names. `compiletime.error` and failed searches are compile errors,
which the lowering cannot report (research 12 §2 e).

## Decision
- `Units::type_unit` runs `Inlining::run` after `zonk_trees` while the `Typer` lives, unless the
  units are `typer_only` (the plain oracle's `-Ystop-after:typer`); the lowering no longer runs it.
- The typer records the `Ctx` of every complete call of an inline method outside one, in `adapt`,
  by its `TNodeId`. Entries of trees speculation drops stay unused; the tree arena never reuses
  ids. Calls an expansion makes use the context of the outermost call.
- `Inlining` keeps its borrowed fields and gets the typer as a `Host`: `Typer::with_inlining`
  moves the universe, trees and names out of the typer (an empty universe in their place) and
  lends them to the expansion; `Host::summon` swaps them back for the search's duration.
- A search inside an inline body that defines givens or implicits, or for a call without a
  recorded context, is refused rather than searched in a context that lacks them.

## Consequences
- Inline expansion errors and missing implicits fall back as type errors or typer refusals;
  `typer-dump --inlined` and the lowering see the same expanded trees.
- The plain typer output is unchanged.
- Rejected: expanding all calls that summon in the typer (dotty expands non-transparent calls
  after the typer, so trees and positions would differ), and making `Inlining` a view of
  `&mut Typer` (every field access of the inliner rewritten).
