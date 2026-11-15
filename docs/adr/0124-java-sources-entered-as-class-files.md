# ADR 0124 — Java sources are entered as the class files javac would write, and compiled by javac

Status: accepted · Date: 2026-11-15 · Amends ADR 0117

## Context
Mixed projects (the Scala 3 compiler has 7 Java files) need Scala code to see Java classes of the
same run, and Java code to see Scala classes. dotty parses Java sources for their declarations
(`JavaParsers`), types them as `JavaDefined` trees with statics in the companion, and leaves the
bodies to javac, which sbt and scala-cli run afterwards against dotty's output. ADR 0117 already
enters Java classes of jars from their class files on lookup.

## Decision
- `scivolo-jvm` takes `.java` files with the Scala ones. A port of dotty's Java scanner and of the
  declaration part of its parser reads them: classes, interfaces, enums, annotation types,
  fields, methods, constructors, type parameters and arguments, nested classes, imports, class
  annotations; bodies and initializers are skipped as dotty skips them, a static final field of a
  literal keeps it as a constant (`tryConstant`).
- Each class is turned, on its first lookup, into the `ClassInfo` javac would write for its
  signatures (access flags, generic signatures, descriptors, inner-class entries, the members
  javac adds: default constructor, enum `values` and `valueOf`), and entered through ADR 0117's
  class-file reader. Names are resolved as in a Java unit: type variables, member classes of the
  enclosing classes and their supertypes, single-type imports, the package (Scala classes of the
  run included), on-demand imports, `java.lang`.
- After the Scala classes are written, javac compiles the Java files into the same directory, with
  that directory and the classpath on its classpath.

## Consequences
- The Java classes get the symbols dotty gives the class files javac writes from the same source
  (case 2 of CLAUDE.md): flags, statics and types match what dotty reads on a separate compile.
  Parameter names are `x$0`, … as for class files without `MethodParameters`.
- Refused by name: records, sealed classes (`sealed`, `non-sealed`, `permits`), receiver
  parameters, inner classes of generic classes and type arguments of an outer class
  (`Outer<A>.Inner`), hexadecimal floating-point literals, names found nowhere, an output jar
  (`-d x.jar`), and a missing javac.
- Annotations of members are dropped; those of classes are kept for the retention of annotation
  types.
