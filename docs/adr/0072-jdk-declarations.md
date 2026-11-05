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
- When JDK jars stand for the JDK (`--jdk-tasty`: nativelib's and javalib's builds), they replace
  whole classes as before, and no declarations are loaded over them.

## Consequences
- A JDK member typed differently by javalib is fixed by declaring it in `JdkDeclarations.scala`
  with its overloads (checked with `javap`), then `just jdk-stubs`.
- A javalib member the JDK lacks is hidden only if a declaration shares its name.
- The JDK's constants (`static final` fields with a `ConstantValue`) are declared as constants:
  `scripts/jdk_constants.py` generates `JdkConstants.scala` from `javap -constants` for the
  classes the other files do not declare and lists the constants those lack. Like
  `JdkMemberDeclarations.scala`, it stays out of `libs/jdk-stubs.jar`. The constants of the
  `java.lang` classes javalib lacks (`StrictMath.PI`) go into `JdkClassConstants.scala`, beside
  `JdkClasses.scala`'s classes.
