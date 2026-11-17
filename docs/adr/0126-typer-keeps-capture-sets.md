# ADR 0126 — The typer keeps capture sets

Status: accepted · Date: 2026-11-17

## Context
Before its `cc` phase dotty's typer never computes a capture set; it carries `@retains`
annotations in its types and drops them only where its own rules act: subtyping strips a
non-refining annotation (`TypeComparer.firstTry` / `secondTry`), so a type variable bound by a
non-singleton type gets no top set, while `ConstraintHandling.widenInferred` widens a singleton
lower bound with its set; `TypeOps.avoid` (`dealiasKeepRefiningAnnots`) drops the top set of a
block's or case's ascription; a closure's parameter and result types are the callee's function
formal. PostTyper's `CleanupRetains` then empties what is left.

Ours drops capture sets when it normalizes a type (`seen_from::normalized`), and the pickler
rebuilds the types dotty's typer gives from the definitions an expression refers to
(`captured_exprs`, `captured_type_args`, `captured_closures`), refusing what it cannot tell. In
the scalalib 2928 inferred types stay untold (105 files refused, #285), and each shape needs
its own re-implementation of dotty's inference in the pickler, which can differ silently
(`id(mk(c))`, fixed by `widened_args`).

## Decision
- In a capture-checked file the typer keeps `@retains` annotations in its types.
- dotty's rules are ported where they act: in subtyping and instantiation (rules 1 and 2), in
  `avoid` for block and case ascriptions (rule 3), and for closure parameters and results.
- The pickler writes the inferred types the typer gives, emptied as `CleanupRetains` does; the
  pickler-side derivation goes where the typer's types supersede it.
- What stays unported is refused: nested sets in patterns, branches that differ only in capture
  sets, dependent function parents, the inferred function type of a closure whose parameters
  have capture sets (`explicifyTpt`'s `@declared`) or whose result's sets name a parameter.

## Consequences
- One home for dotty's inference rules; a shape becomes told by porting the rule that types it.
- Files without capture checking keep today's types: typer snapshots and NIR stay the same.
- Every place that compares or erases types of a capture-checked file must see through
  `@retains` (as subtyping already does); each newly accepted shape is checked against dotty's
  TASTy.
