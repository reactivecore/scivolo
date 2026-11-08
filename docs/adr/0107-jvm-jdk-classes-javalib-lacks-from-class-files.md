# ADR 0107 — On the JVM, JDK classes javalib lacks are entered from the JDK's class files

Status: accepted · Date: 2026-11-08 · Amends ADR 0106

## Context
The typer knows the JDK through javalib's TASTy and the JDK declarations (ADR 0072/0082).
javalib lacks whole packages (`java.time`, `java.text`) and classes (`ConcurrentSkipListMap`),
which dotty reads from the JDK's class files (`ClassfileParser`). ADR 0106 found those files.

## Decision
- A universe has a JDK only for the JVM backend (`Universe::set_jdk`); the NIR path has none.
- A name a `java.*` or `javax.*` package (or the root, for `java`, `javax`) lacks in the library,
  its files and its package objects is looked up in the JDK last: a JDK package is entered, a
  public top-level class that is no annotation is entered with its object and object's class.
- The class's type parameters and parents come from its `Signature`, else from its superclass
  and interfaces; a member's type from its `Signature`, else its descriptor, as dotty's
  `sigToType` reads them (`Object` as `<FromJavaObject>`, varargs `Array[T] @Repeated`, a
  constructor polymorphic in the class's type parameters). Static members go to the object's
  class, as for the Java classes of the Scala library. Private, package-private, synthetic and
  bridge members are not entered. A `static final` field's `ConstantValue` is its constant type.
- A method's erasure is its descriptor's.
- Not entered (unknown to the typer, so refused as a member not found): classes nested in such a
  class, annotation classes, and members whose signature names a class nested in a generic one.

## Consequences
- The library still wins: a JDK member javalib lacks in a class javalib has stays unknown.
- Differences from dotty: no annotations, `Exceptions` or `InnerClasses` read; a raw type in a
  descriptor stays unapplied (dotty's `cook` gives it wildcards); enums read are not sealed, so
  matches on them get no exhaustivity warnings.
