# ADR 0025 — Implicit ranking refuses where dotty's outcome is order- or version-dependent

Status: accepted · Date: 2026-10-14

## Context
Dotty ranks successful implicit candidates by `compare` (`Applications.scala`) inside `rank`
(`Implicits.scala`), which tries candidates one by one in its own order and drops those worse
than the current winner. Under `-source 3.6` and `3.7-migration` it ranks with both the old and
the new prioritization rules, keeping the better of the two while ranking and one of them when
disambiguating. Our search tries every candidate of a level and ranks the successes afterwards.

## Decision
- **Winner:** a candidate wins only if `compare` ranks it above every other success; otherwise
  the search is refused as ambiguous. Where dotty's sequential `rank` would pick a winner only
  through its candidate order, this refuses instead of choosing.
- **Undecided comparisons:** `compare` returns undecided for what we do not model (a type
  constructor whose parameter variances are unknown, results depending on an implicit
  parameter, repeated parameters, two equal references to one stable value); an undecided
  pair makes the search ambiguous.
- **Source versions:** `3.7` and later use the new rules, versions before `3.6` the old ones.
  Under `3.6` and `3.7-migration` both rule sets must give the same result for every pair,
  else the search is refused.

## Consequences
- Every departure shows as a refusal (unsupported), never as a different winner.
- Some programs dotty accepts stay unsupported: ties dotty breaks by order, and searches that
  only one of the two rule sets decides under `3.6`.
