# ADR 0071 — Synthetic members added before the inlining phase

Status: accepted · Date: 2026-11-01

## Context
dotty adds `SyntheticMembers` (case class methods, mirror support, `writeReplace`) in PostTyper,
before `Inlining` expands non-transparent inline calls and macros; a macro sees them in its
arguments and in `Symbol.declarations` of the unit's classes (research 44 §2). Our inlining phase
runs in `Units::finish`, but the port of `SyntheticMembers` runs in the lowering, after it
(`crates/lower/src/phases/synthetic.rs`), so macros see the typer's classes.

## Decision
- The lowering's `synthetic::add_synthetic_members` runs on the whole unit before the inlining
  phase, through a hook on `typer::Units` that the lowering's drivers set; the typer does not
  depend on the lowering, and `typer_only` units (typer-compare) stay dotty-typer trees.
- The lowering runs the same pass again and adds only what is missing: classes made or copied
  while inlining. Every part of the pass skips members and parents that exist (dotty's
  `existingDef`), so no class gets a member twice.
- Fresh names the pass takes follow dotty's order: PostTyper, then Inlining, then PostInlining.

## Consequences
- Macros, `declarations` and NIR agree on the members of a class; research 44 §5 is the plan.
- A phase that runs before `synthetic` in the lowering (super accessors, companion order) still
  runs after macros; dotty's `wrapTemplate` precedes `addSyntheticMembers`. Accepted for now.
