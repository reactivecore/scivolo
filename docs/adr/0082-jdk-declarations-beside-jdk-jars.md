# ADR 0082 — JDK declarations beside JDK jars, of renamed classes and type parameters

Status: accepted · Date: 2026-11-03 · Extends ADR 0072

## Context
ADR 0072 loads no JDK declarations once JDK jars are given (`--jdk-tasty`). scalalib's build
gives JDK jars only for the classes javalib lacks, so it saw javalib's `Byte.TYPE`
(`Class[PrimitiveByte]`), `Boolean.TRUE` (a `def`, unstable in a pattern) and `Arrays.copyOf[T <:
AnyRef]` where Scala Native compiles against the JDK. javalib's `SoftReference[T >: Null <:
AnyRef]` and nativelib's `private[runtime] object _Class` (`Class.forName`) differ from the JDK
in the class itself, not in a member. nativelib's build takes the declared classes whole from
`libs/jdk-stubs.jar`, so a class declared with a few members hides javalib's other members there.

## Decision
- The JDK's declarations are loaded over a file unless javalib's sources are compiled or the
  file is a JDK jar's; JDK jars still replace whole classes.
- `JdkMemberDeclarations.scala` holds declarations that are not whole enough for nativelib's
  build; `just jdk-stubs` leaves them out of `libs/jdk-stubs.jar`, so nativelib's build sees them
  over javalib's classes as every other build does.
- A declared class or object of a class Scala Native renames is declared under its defined name
  (`scala.scalanative.runtime._Class`) and merged into the class entered under the JDK's name.
- A merged class or object is public, as the JDK's.
- A merged type parameter takes the declaration's bounds. A Java constructor's type argument
  is checked against its class's type parameter where that is unbounded, since javalib's
  constructor keeps javalib's bounds.
- The `Object` bound of any Java member's type parameter admits any type (dotty's
  `FromJavaObject`), not only a constructor's. A declared type parameter is `<: Object` where an
  array of it erases to `Object[]` (`Arrays.copyOf`), else unbounded.
- A Java `static final` field is a declared `val`, so stable, as in dotty (`isStableMember`).
- The term parameters of JDK methods are named `x$0`, `x$1`, …, as dotty names those of class
  files without parameter names.
- A wildcard `? <: Any` in a JDK member's type is `? <: <FromJavaObject>`, dotty's alias of
  `Object` that every type conforms to.

## Consequences
- JDK classes that scalalib needs and javalib lacks go into `JdkClasses.scala` when a
  declaration refers to them (`java.lang.reflect.Field`), else into `JdkStubs.scala`.
- The `Object` bound of a Java type parameter and an `Object` parameter stay `Object`, admitting
  any type in the typer only; dotty has `<FromJavaObject>` there too.
