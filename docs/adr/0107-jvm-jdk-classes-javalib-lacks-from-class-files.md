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
  public top-level class is entered with its object and object's class (an annotation class as
  ADR 0117 says).
- The class's type parameters and parents come from its `Signature`, else from its superclass
  and interfaces; a member's type from its `Signature`, else its descriptor, as dotty's
  `sigToType` and `cook` read them (`Object` as `<FromJavaObject>`, a raw type applied to
  wildcards, varargs `Array[T] @Repeated`, a constructor polymorphic in the class's type
  parameters). Static members go to the object's class, as for the Java classes of the Scala
  library. Private, package-private, synthetic and bridge members are not entered. A field's
  `ConstantValue` is its constant type. An enum with constants is sealed and abstract.
- A method's erasure is its descriptor's.
- Not entered (unknown to the typer, so refused as a member not found): classes nested in such a
  class, signature-polymorphic methods (native varargs), constructors with
  type parameters of their own, and members whose signature names a class nested in a generic
  one.

## Consequences
- The library still wins; a JDK member javalib lacks in a class javalib has is entered as ADR
  0108 says.
- No annotations other than a class's `@Retention` (ADR 0117), no `Exceptions` or `InnerClasses`
  are read: an eta-expansion to such a class is
  refused, as for a Java class that is no known functional interface; deprecation warnings and
  an enum's children (for its sum mirror, refused) are missing.
