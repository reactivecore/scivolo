# ADR 0106 — The JVM backend reads the JDK's classes from its run-time image or ct.sym

Status: accepted · Date: 2026-11-08

## Context
Stack map frames need the JDK's class hierarchy, and a static member of javalib's object for a
JDK class must exist in the JDK. The JDK's class files are in `lib/modules` (jimage, the run-time
image `java` loads them from, in every JDK; resources may be compressed by `jlink --compress`),
`jmods/` (absent from Debian's headless JDK) or `lib/ct.sym` (stripped `.sig` class files of the
earlier releases only, so JDK N's has N−1's API). dotty reads the running JDK's classes (jrt), scala-cli adds no `-release`.

## Decision
- The JDK is that of `JAVA_HOME`'s `java`, else of the `java` on the `PATH`, links resolved.
- Its classes come from `lib/modules`, its uncompressed class files; if it lacks an uncompressed
  `java/lang/Object`, from `lib/ct.sym`'s latest release. No decompression, no jmods.
- The frame LUB takes a class of a `java` or `javax` package from the JDK, falling back to the
  universe; any other class from the universe, falling back to the JDK.
- A member of an object standing for a JDK class is a static member of that class. A value of the
  JDK declarations (`@static`) or of a Java class entered from the library jars is its static
  field. Another is a static field if it is parameterless and the JDK's class or a supertype has a
  public or protected static field of its name and descriptor; else a static method if the class,
  or for a class a superclass, has such a method; else it is refused.
- Without a readable JDK (no `java`, or a jlinked runtime with a compressed `lib/modules` and no
  `lib/ct.sym`), `MethodHandles.lookup`, `Lookup.findVarHandle` and `VarHandle.compareAndSet` are
  known from a table in the reader: every thread-safe lazy value and `given` alias calls them, and
  their class files come out the same as with a JDK. A readable JDK lacking them refuses those.
- **Not taken:** a generated table of the JDK 17 classes' static fields, the earlier state: it
  says nothing about methods, and fails at run time for a member the JDK lacks.
- **Not taken:** refusing lazy values without a readable JDK: programs that use nothing else of
  the JDK would stop compiling.

## Consequences
- With a compressed `lib/modules` the reader sees ct.sym's release N−1 of JDK N: a class or
  static member new in N is refused.
- Without a JDK, programs calling a JDK class's static members are refused, but for the three
  above.
