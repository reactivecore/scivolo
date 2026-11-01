# ADR 0065 — Quote patterns typed as `QuotePattern` and `SplicePattern` trees

Status: accepted · Date: 2026-11-01

## Context
dotty's typer gives a quote pattern `case '{ $x: T }` a `QuotePattern` tree of its type
bindings, its body with `SplicePattern` trees in place of the splices, and the `Quotes` it
matches with (`typedQuotePattern`). Only `CrossStageSafety` in the staging phase encodes it
(`QuotePatterns.encode`) as the `QuoteMatching.ExprMatch.unapply` call whose shape quote holds
`patternHole[T]` calls; that is the form compiled code and pickled shapes have, which the
unpickler (`unpickle/quote_patterns.rs`) and `inlining::quote_matcher` read (ADR 0029). The
encoding drops the ascription a splice pattern gets (`Typed(SplicePattern, tpt)`), so the
typed trees cannot be printed back as dotty's from it.

## Decision
- The typed trees have `QuotePattern { bindings, body, quotes }` and
  `SplicePattern { body, typeargs, args }` in the field order of dotty's classes; the typer
  makes them as `typedQuotePattern`, `typedSplicePattern` and `typedQuotedTypeVar` do.
- The encoded form is made where dotty makes it, by the port of `QuotePatterns.encode` in the
  staging step (research 43, step 4), so the pickled shape is the one the unpickler and the
  matcher already read. Until then the lowering refuses quote patterns outside inline
  methods (#199).

## Consequences
- Tree walks see two more classes; those that only look at children need no change.
- A future `QUOTEPATTERN` unpickler can read into the same trees.
