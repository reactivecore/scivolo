# ADR 0072 — JDK classes typed from the JDK's declarations

Status: accepted · Date: 2026-11-02

## Context
Scala Native compiles user code against the JDK's class files; we compile it against javalib's
TASTy. Where javalib declares a JDK class differently, the typing and the NIR differ: `List.size`
resolves to `Collection`, javalib's inherited declaration (#195); `Integer.TYPE` is javalib's
`final val TYPE = classOf[PrimitiveInt]`, folded to a constant and typed `Class[PrimitiveInt]`
where the JDK has a static `Class[Integer]` (#191). Only nativelib's library build saw the
JDK's declarations, as whole classes in place of javalib's (`--jdk-tasty libs/jdk-stubs.jar`).

## Decision
- `libs/jdk-stubs/JdkDeclarations.scala` holds the JDK's declarations of members of classes
  javalib defines; `just jdk-stubs` writes their TASTy into `libs/jdk-declarations.jar` under
  `jdk-declarations/`, outside the package table. Every frontend's library has it: the library
  image (scripts, `scivolo-aot`, the REPL) and `typer::dump::library_jars` (the tools and tests).
- Loading a file of the library loads its JDK declarations too, if any (`load_jdk_declarations`):
  their classes, objects and type parameters are javalib's symbols; each member they declare
  replaces javalib's members of its name in that class, so a name lists all the JDK's overloads
  of its class. Constructors, synthetic members and classes javalib lacks are not entered.
- A declared repeated parameter `T*` is Java varargs, `Array[T] @Repeated`, as is one of a JDK
  jar's (`--jdk-tasty`, e.g. `libs/jdk-stubs.jar`).
- `libs/jdk-stubs/JdkClasses.scala` holds JDK classes javalib lacks that library sources import
  (`java.io.SequenceInputStream`); their TASTy is in `libs/jdk-declarations.jar` in the package
  table, without NIR, so a use fails to link, as in Scala Native.
- The values of javalib's objects of JDK classes are static, as their methods are: Java's enum
  constants and static fields (`TimeUnit.SECONDS`) are `TimeUnit::static SECONDS()`.
- Bodies stay javalib's: the VM runs javalib's NIR. NIR calls keep the JDK's declaring class and
  the JDK's static members (`Integer::static TYPE()`), which javalib's static forwarders define.
- JDK jars (`--jdk-tasty`) replace whole classes, and no declarations are loaded over their files;
  declarations are loaded over every other file unless javalib's sources are compiled. So scalalib's
  build, which gives JDK jars only for the classes javalib lacks, sees the JDK's `Byte.TYPE`,
  `Boolean.TRUE` and `Arrays.copyOf`.
- `JdkMemberDeclarations.scala` holds declarations not whole enough to replace a class:
  `just jdk-stubs` leaves them out of `libs/jdk-stubs.jar`, so nativelib's build sees them over
  javalib's classes too.
- A declared class or object Scala Native renames is declared under its defined name
  (`scala.scalanative.runtime._Class`) and merged into the class entered under the JDK's name. A
  merged class or object is public, a merged type parameter takes the declaration's bounds.
- JDK members read as dotty reads class files. A `static final` field is a declared `val`, so
  stable (`Boolean.TRUE` in a pattern, dotty's `isStableMember`). The parameters of JDK methods are
  `x$0`, `x$1`, … A wildcard `?` in a JDK member's type is bounded by `<FromJavaObject>`, dotty's
  alias of `Object` that every type conforms to (`Universe::with_java_wildcards`).
- A declared type parameter is `<: Object` where an array of it erases to `Object[]`
  (`Arrays.copyOf`), else unbounded. The `Object` bound of a Java member's type parameter and an
  `Object` parameter stay `Object` and admit any type in the typer only (`Typer::instantiated`,
  `Typer::from_java_object`), where dotty has `<FromJavaObject>`. A Java constructor's type
  argument is checked against its class's type parameter where that is unbounded.
- **Not taken:** whole classes in place of javalib's for every build, as `libs/jdk-stubs.jar` gives
  nativelib: a class declared with a few members would hide javalib's others.

## Consequences
- A JDK member typed differently by javalib is fixed by declaring it in `JdkDeclarations.scala`
  with its overloads (checked with `javap`), then `just jdk-stubs`.
- A javalib member the JDK lacks is hidden only if a declaration shares its name.
- A method javalib declares in a class whose JDK class inherits it is called on the JDK's
  declaring superclass (`WeakReference.get` on `Reference`): `scripts/jdk_inherited.py` lists
  them by name from javalib's NIR and JDK 21's classes.
- The JDK's constants (`static final` fields with a `ConstantValue`) are declared as constants:
  `scripts/jdk_constants.py` generates `JdkConstants.scala` from `javap -constants` for the
  classes the other files do not declare and lists the constants those lack. Like
  `JdkMemberDeclarations.scala`, it stays out of `libs/jdk-stubs.jar`. The constants of the
  `java.lang` classes javalib lacks (`StrictMath.PI`) go into `JdkClassConstants.scala`, beside
  `JdkClasses.scala`'s classes.
