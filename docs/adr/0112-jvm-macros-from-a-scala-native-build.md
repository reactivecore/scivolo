# ADR 0112 — On the JVM, macros run in the VM over a Scala Native build of their groups

Status: accepted · Date: 2026-11-09

## Context
Our typer expands a macro call by running the macro implementation in the VM (ADR 0029), which
needs its NIR. dotty's tests of macros compile the implementation in an earlier group; the JVM
report compiled earlier groups with scala-cli for the JVM only, so `scivolo-jvm` refused every
macro call.

## Decision
- `scivolo-jvm --macro-cp DIR` runs macro calls in a VM over the library's NIR and `DIR`'s; the
  typer still reads the TASTy of `--cp`.
- `scripts/dotty_jvm.py` compiles the earlier groups of a test twice when the last group calls a
  macro: for the JVM (class files and TASTy, the typer's classpath and the run's) and for Scala
  Native (NIR, the macro classpath; the build `scripts/dotty-lower.py` keeps).
- A last group with macro definitions the typer refuses is split as `scripts/dotty-lower.py`
  splits it: the files that mention `scala.quoted` first, as dotty suspends their callers.
- `just programs-jvm` runs `tests/programs` on the JVM against their `.check` files.

## Consequences
- The typer sees the JVM build's TASTy while the macro runs from the Scala Native build's NIR;
  both come from scala-cli 3.9.0 of the same sources.
- A macro that reflects on the platform (`System.getProperty`, class names) sees Scala Native.
