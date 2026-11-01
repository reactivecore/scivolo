# ADR 0057 — `$N$` names numbered in the order the typer meets them

Status: accepted · Date: 2026-11-01

## Context
dotty takes the `$N$` names of pattern definitions (`val (a, b) = e`), pattern generators and
the values the typer lifts (`a(i) += 1`) from one counter per compilation unit
(`UniqueName`). It desugars a pattern definition when the namer enters the statements of its
block or template, a `for` when the typer reaches it; the typer lifts while it types. Our
desugaring runs over the whole file before typing, so a pattern definition took its numbers
before every lift of the file: BigInteger, regex.Parser and others named them out of
dotty's order.

## Decision
- The desugaring keeps numbering `$N$` names as it goes; `FreshNames` records which names
  `unique` made and groups the names of one pattern definition, and those of one `for`.
- The typer records each symbol defined with such a name: entered by the namer, bound by a
  pattern, lifted. Defining one counts its group as met, in the group's order.
- Once the units are typed, before inlining, the recorded symbols and the trees that define
  or refer to them are renamed `$1$`, `$2$`, … in the order their names were met.

## Consequences
- A pattern definition counts as met when the namer enters its first definition: at its
  block's or template's entry, as in dotty. One without a `val $N$` of its own (one variable
  or none) and a `for` count when the typer types their first binder, after the scrutinee
  or the first generator: a lift there takes the lower number, dotty's the higher.
- Inlined copies of a script's inline definitions take the renamed names; the counter goes
  on from the desugaring's count, so later `$N$` names do not clash with renamed ones.
