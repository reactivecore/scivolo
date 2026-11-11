# ADR 0117 — On the JVM, JDK annotation classes javalib lacks are entered from class files

Status: accepted · Date: 2026-11-11 · Amends ADR 0107

## Context
Scala code annotates definitions with JDK annotations (`@FunctionalInterface`, 87 files of the
Scala library). javalib lacks them, and ADR 0107 did not enter annotation classes. dotty enters a
Java annotation as a trait with a synthetic constructor of its elements
(`addAnnotationConstructor`) and the backend keeps it in the class file unless its `@Retention`
is `SOURCE` (`shouldEmitAnnotation`).

## Decision
- An annotation class of a `java.*` or `javax.*` package javalib lacks is entered as ADR 0107
  enters a class, with dotty's constructor: a parameter per element, of the element's type.
- Its `@Retention` is read from its class file (`CLASS` without one).
- The typer creates it only as the `new` of an annotation of a definition; elsewhere, also in
  that annotation's arguments, `new` of it stays an error, as in dotty. Its constructor is
  pickled by name, unsigned, as dotty pickles it.
- The backend writes an annotation of `RUNTIME` retention to `RuntimeVisibleAnnotations`, of
  `CLASS` retention to `RuntimeInvisibleAnnotations`, where dotty does: on classes and mirror
  classes, methods, static and mixin forwarders, and a value's field rather than its getter and
  setter (`Memoize`); not on trait static forwarders and `$init$`.

## Consequences
- Refused: an annotation with arguments (dotty's named arguments and `_` defaults are not built;
  one with a defaulted element and no arguments is refused by the typer as a default getter),
  several kept Java annotations on one definition (their order is not settled), and kept Java
  annotations of parameters (`RuntimeVisibleParameterAnnotations` is not written).
- Java annotations of a jar on `--cp` are #239's next step.
