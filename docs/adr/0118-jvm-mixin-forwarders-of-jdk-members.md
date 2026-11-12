# ADR 0118 — On the JVM, mixin forwarders of JDK members follow the JDK's class files

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

## Consequences
- The typer still sees javalib's members and types: a call of a javalib-only public member
  compiles and fails at run time; overrides keep their own types.
- The NIR path, without a JDK, is unchanged.
