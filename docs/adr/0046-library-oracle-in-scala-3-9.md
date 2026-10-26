# ADR 0046 — The library oracle compiled with Scala 3.9.0

Status: accepted · Date: 2026-10-26

## Context
`just lib-compile` compares the NIR the frontend makes of each Scala Native library source with
an oracle, Scala Native's own sbt build of the module (ADR 0042). That build uses Scala 3.8.3,
the newest version its compiler plugin's 0.5.12 sources compile against, while the frontend
types as 3.9.0. Much of what differed was 3.8 against 3.9, not a frontend bug: pattern binders
(`x2` against the binder's name), `IndexOutOfBoundsException(Int)` in the synthetic
`productElement`, names of extracted variables. Scala Native publishes the plugin compiled for
3.9.0 (`nscplugin_3.9.0`), which scala-cli uses for the test programs.

## Decision
- `just lib-oracle` builds a copy of `ref/scala-native` in `tmp/lib/build` with `++3.9.0!`,
  the published `nscplugin_3.9.0` in place of the plugin's sources, and without `-Werror`
  (3.9 warns where 3.8 did not). The patch is two substitutions in the copy's `project/`; the
  script fails if either does not apply.
- `ref/scala-native` itself stays untouched, shared by the checkouts.

## Consequences
- The oracle and the test programs are compiled by the same compiler and plugin; a difference
  is the frontend's, or a position or name the frontend gives differently.
- The NIR is not byte for byte what the 0.5.12 release ships: that is Scala 3.1.3's, the build's
  3.8.3's differs too. Matching 3.9.0's is the goal the frontend can reach.
- A newer Scala Native whose plugin sources compile against 3.9 makes the patch unnecessary.
