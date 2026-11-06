# ADR 0094 — TailRec variables are captured by value before LambdaLift

Status: accepted · Date: 2026-11-06

## Context
dotty runs `TailRec` before `LambdaLift` and substitutes the `$tailLocal` variables of a
rewritten method in its whole body, closures, local methods and local classes included.
`CapturedVars` collected the captured variables before `TailRec` made them, so they stay unboxed
and `LambdaLift` passes them by value: `reverseOnto$$anonfun$1($this$tailLocal5$1, tl$tailLocal1$1)`,
static because it reads the receiver variable, not `this`. Our `tailrec` rewrites members after
`lambda_lift` (#67).

## Decision
- Before `lambda_lift`, `tailrec::name_vars` makes local tail-recursive methods loops and names
  the variables of members' tail-recursive methods; the methods and classes nested in such a
  member read those variables.
- These variables are `tail_locals`: `lambda_lift` and `local_classes` capture them by value,
  `captured_vars` (which runs earlier) never boxes them.
- `tailrec::transform` makes the members loops after `lambda_lift`, defining the named variables.

## Consequences
- A closure made in an iteration sees the parameter's value of that iteration, as in Scala
  Native (`tests/programs/tailcaptures`).
- The NIR of such closures and local classes matches Scala Native's in names, parameters and
  staticness.
