# ADR 0106 — The JVM backend reads the JDK's classes from its jmods or ct.sym

Status: accepted · Date: 2026-11-08 · Amends ADR 0103

## Context
Stack map frames need the JDK's class hierarchy, and a static member of javalib's object for a
JDK class must exist in the JDK (ADR 0103 kept a table of the JDK 17 static fields). The JDK's
class files are in `jmods/` (absent from Debian's headless JDK, CI's), `lib/ct.sym` (stripped
`.sig` class files of the earlier releases) or `lib/modules` (jimage, with compressed resources).
dotty reads the running JDK's classes (jrt), scala-cli adds no `-release`.

## Decision
- The JDK is that of `JAVA_HOME`'s `java`, else of the `java` on the `PATH`, links resolved.
- Its classes come from `jmods/` (`java.base` first, the other jmods opened on a miss), else from
  `lib/ct.sym`'s latest release. No jimage reader.
- The frame LUB takes a class of a `java` or `javax` package from the JDK, falling back to the
  universe; any other class from the universe, falling back to the JDK.
- A member of javalib's object for a JDK class is a static field if it is parameterless and the
  JDK's class or a supertype has a static field of its name and descriptor; else a static method
  if the class or a superclass has one of its name and descriptor; else it is refused. The table
  of ADR 0103 is gone.

## Consequences
- On a JDK without jmods the reader sees release N−1 of JDK N: a class or static member new in N
  is refused.
- Without a JDK, programs calling a JDK class's static members are refused.
- JDK classes javalib lacks are still unknown to the typer: entering them from the class files
  (dotty's `ClassfileParser`, research 46 §5.2) is the next step.
