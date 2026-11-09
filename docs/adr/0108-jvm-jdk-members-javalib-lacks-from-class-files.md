# ADR 0108 — On the JVM, JDK members javalib lacks are entered from the JDK's class files

Status: accepted · Date: 2026-11-09 · Amends ADR 0107

## Context
ADR 0107 enters the JDK classes javalib lacks from the JDK's class files, but a JDK member
javalib lacks in a class javalib has stays unknown: `Class.getDeclaredMethods`,
`EnumSet.allOf`, `Field.getModifiers`. dotty reads every JDK class from its class file.

## Decision
- With a JDK (`Universe::set_jdk`, the JVM backend), the first failed lookup of a name in a
  `java.*` or `javax.*` class of the library other than `Object` enters, once per class, the
  public and protected members of its JDK class file that are named as no member of the class
  (its own or a base class's, from the library) is: instance members into the class, static
  ones into its object's class. Base classes get theirs first; a JDK member a base class got
  is overridden by the class's own (`Enum.describeConstable` over `Constable`'s).
- A member is read as ADR 0107 reads one (`Signature`, else descriptor; erasure the
  descriptor's; concrete unless `ACC_ABSTRACT`); the JDK class's type parameters stand for
  the library class's (`C.this.A`), by position; a different count enters nothing. Classes
  the members name that javalib lacks resolve through ADR 0107.
- A name of which one member is refused (signature polymorphic, a class nested in a generic
  one) is not entered, so no overload set is partial.

## Consequences
- Members javalib declares keep javalib's type; javalib's overloads of a name are never joined
  by the JDK's, and a JDK override of a library base class's member is not entered (the base's
  answers).
- Classes javalib lacks altogether stay ADR 0107's: nested ones (`MethodHandles.Lookup`) and
  packages outside `java.*`/`javax.*` (`sun.misc.Unsafe`) are unknown; JDK annotation classes
  (`Deprecated`) too, as the typer has no Java annotation constructor.
- The NIR path, without a JDK, is unchanged.
