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
- The desugaring keeps numbering `$N$` names as it goes. `FreshNames` records the names of
  each pattern definition under the start offsets of the definitions it becomes, and those of
  each `for` under the start of the `for`.
- The typer meets a pattern definition's names when the namer enters a statement starting
  there, a `for`'s when it types a tree starting there, and a lifted or otherwise defined
  `$N$` name when it defines its symbol. It records each symbol defined with such a name.
- Once the units are typed, before inlining, the recorded symbols and the trees that define
  or refer to them are renamed `$1$`, `$2$`, … per source file, in the order their names
  were met.

## Consequences
- The source files of one unit each number from `$1$`, as dotty's compilation units do.
- Inlined copies of a script's inline definitions take the renamed names; the counter goes
  on from the desugaring's count, so later `$N$` names do not clash with renamed ones.
- Macros see the desugaring's numbers while they expand; `$N$` names a macro makes are not
  renamed.
