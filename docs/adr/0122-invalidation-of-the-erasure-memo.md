# ADR 0122 — Invalidation of the erasure memo

Status: accepted · Date: 2026-11-15

## Context
`Universe::erased_method` remembers the erasure of each method until the universe changes
(`changed`). Erasing a reference to a class recomputed its erasure and binary name each time:
a walk over its owners and, for an object's class, a scan of its owner's declarations for a
`@targetName`. Keeping the memo over new methods and values by recording the names erasures
look up gained under 0.3% of instructions on upickle-core and rested on the claim that no
erasure depends on a declaration in any other way.

## Decision
- The memo (`ErasureMemo`) also holds the erasures of references to classes other than value
  classes and the binary names of classes. All of it is dropped by every change that dropped
  the method memo before, and also by defining a symbol with a run-time name (`@targetName`),
  setting one (`set_target`), and defining a symbol without TASTy other than a method or a
  value that is not a class parameter's accessor (erasure scans declarations for those:
  `underlying`, `class_target`, opaque types, type parameters).
- An entry is kept only if no change happened while it was computed (`generation`).
- `class_target` scans an owner's declarations for the run-time name of an object only if the
  owner ever had a declaration with one (`may_declare_targets`).
- Debug builds recompute each remembered erasure and binary name on use and assert that it is
  equal.

## Consequences
- A class's erasure and binary name depend on its owners, names, flags, kinds, parents,
  run-time names and declarations, as a method's erasure does; the memo is dropped as the
  method memo was, so it is valid as long as that was.
- Tests run every remembered erasure twice; a stale entry fails them instead of erasing wrong.
