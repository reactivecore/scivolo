# ADR 0122 — Invalidation of the erasure memo

Status: accepted · Date: 2026-11-15

## Context
`Universe::erased_method` remembers the erasure of each method until the universe changes.
Every change (`changed`) dropped it, and `define` changes the universe on almost every new
member. Lowering defines many methods and values (bridges, forwarders, accessors), renames
locals and unenters bridges, so the memo was empty most of the time. dotty caches an erasure per denotation and period
(`SymDenotation.info` at `erasurePhase`); a new member does not end a period.

## Decision
- The memo (`ErasureMemo`) also records the names the erasures it holds looked up: every
  by-name lookup of declarations (`Universe::named`) and every remembered resolution
  (`resolve_ref`, with its prefix's names) while an erasure is computed (`while_erasing`).
- Defining, entering, unentering or renaming a method, or a value that is neither an object
  nor a class parameter's accessor, keeps the memo unless its name (old or new) is one of those
  names. Every other change drops it as before: other kinds of symbols, set infos, flags,
  parents, self types, kinds.
- Debug builds recompute each remembered erasure on use and assert that it is equal.

## Consequences
- A method or value can change an erasure only through a lookup of its name (a `TermRef`, a
  member of a refinement) or as a value class's underlying value, which a class parameter's
  accessor is; both are covered.
- The memo stays per `Universe` (one unit), so it keeps parallel compilation of files open.
- Tests run every remembered erasure twice; a stale entry fails them instead of erasing wrong.
