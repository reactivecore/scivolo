# ADR 0118 — On the JVM, JDK members and their mixin forwarders follow the JDK's class files

Status: accepted · Date: 2026-11-12 · Amends ADR 0108

## Context
JDK interfaces javalib has come from javalib's TASTy (ADR 0108), whose members differ from the
JDK's: helpers the JDK lacks (`Map.usingEntry`, `protected[util]`) and other types
(`Comparator.thenComparing[U <: Comparable[U]]` where the JDK has `Comparable<? super U>`).
dotty reads the JDK's class files, so its mixin forwarders forward only the JDK's members, with
their generic signatures.

## Decision
- With a JDK (the JVM backend), a member of a library `java.*` or `javax.*` class is in the JDK
  only if its JDK class file declares a public or protected member, neither synthetic nor a
  bridge, of its name, staticness and erasure (`Universe::jdk_lacks`). No mixin forwarder is
  made for one that is not, as for the JDK 17 view of the NIR library compile (ADR 0095).
- A mixin forwarder of a JDK member takes its type from that declaration (`Universe::jdk_info`),
  the JDK class's type parameters standing for javalib's by position; javalib's type when the
  signature is refused.
- The typer sees the same: a lookup by name hides the library's methods and values the JDK
  lacks (`Universe::jdk_lacking`), and enters the JDK's members of that name where none is
  left; a method the JDK has, but a constructor, takes the declaration's type.

## Consequences
- A call of a javalib-only member is refused as "member not found", as dotty reports it.
- Members are checked when first looked up by name; code that walks a class's declarations
  without a lookup (overriding checks) still sees javalib's.
- Values keep javalib's types, so their constants still fold.
- The NIR path, without a JDK, is unchanged.
