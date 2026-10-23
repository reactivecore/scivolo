# ADR 0037 — Implicit searches of a macro answered between runs of the expansion

Status: accepted · Date: 2026-10-23

## Context
`Implicits.search` and `Expr.summon` (mainargs' `TokensReader`s) need the typer's implicit
search while the macro runs. The typer has lent its tables to the expansion (`Lent`), and the
macro's host is a `'static` trait object the VM holds (ADR 0029), so it cannot borrow the
typer to call back into it; research 31 §3.3 proposed lending a boxed closure, which that rules
out without unsafe code.

## Decision
- A search the expansion's earlier runs did not answer records its type on the host and
  refuses the run (`MacroError::Search`). The splice searches the type in the call's context
  (`Inlining::summon`) and runs the macro again with the answer in `MacroCall::searched`; a
  found argument is a tree the macro gets as an `Expr`, none a `NoMatchingImplicits` failure.
- A macro implementation is deterministic in its inputs, so each run asks the same searches in
  the same order; the types are interned, so the answers match by `TypeId`. An expansion runs at
  most 64 times; after that it is refused.
- Ambiguous and diverging searches refuse the expansion, as the typer's search does.

## Consequences
- An expansion with `n` searches runs `n + 1` times; the trees and symbols of the runs before
  the last are left unused in the unit's tables.
- The typer is never re-entered while the VM runs Scala code.
