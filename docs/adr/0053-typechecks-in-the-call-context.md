# ADR 0053 — `typeChecks` types its code in the context of the call

Status: accepted · Date: 2026-10-26

## Context
`scala.compiletime.testing.typeChecks(code)` and `typeCheckErrors(code)` are intrinsics: dotty
parses the constant string `code` as a block, types it in the context of the call with a fresh
typer state, runs `PostTyper`, the inlining and `RefChecks` on it, and answers whether errors
were reported (`Inlines.Intrinsics.compileForErrors`). The call is a transparent inline method,
so it expands while the typer runs (ADR 0020); the inlining has the typer's tables lent, and the
typer's context of the call only through `Host::call_ctx`.

## Decision
- **Through the host.** The inlining hands the code to `Host::typed_code` with the context of the
  outermost call. The typer parses it into its own `Ast` (as the REPL's wrapper does), desugars
  and types it as a block owned by a synthetic value of the context's owner, and drops the
  diagnostics it reported; the trees and symbols made stay unused.
- **Then a full inlining.** A second `Inlining`, not limited to transparent calls and with the
  unit's inline bodies known so far, expands the typed code, so `compiletime.error` in it counts;
  `Host::errors_since` takes back the errors that reported.
- **Refusals stay refusals.** Whatever the typer or the inlining refuses in the code refuses the
  program, also where dotty would answer `false`: our typer refuses some input dotty reports as
  an error (an unresolved name, a missing implicit), and answering `false` there could be wrong.
- **`typeCheckErrors`** answers `Nil` for code without errors and is refused otherwise, since
  our messages and positions differ from dotty's.

## Consequences
`typeChecks` is exact where the frontend types and inlines the code: an answer `true` or
`false` from errors both compilers report. Code calling an inline method of the unit that is not
typed yet when the transparent call expands is refused. `RefChecks`-only errors are those the
typer reports itself.
