# ADR 0124 — Java sources are entered as the class files javac would write, and compiled by javac

Status: accepted · Date: 2026-11-15 · Amends ADR 0107

## Context
Mixed projects (the Scala 3 compiler has 7 Java files) need Scala code to see Java classes of the
same run, and Java code to see Scala classes. dotty parses Java sources for their declarations
(`JavaParsers`), types them as `JavaDefined` trees with statics in the companion, and leaves the
bodies to javac, which sbt and scala-cli run afterwards against dotty's output. ADR 0107 already
enters Java classes of jars from their class files on lookup.

## Decision
- `scivolo-jvm` takes `.java` files with the Scala ones. A port of dotty's Java scanner and of the
  declaration part of its parser reads them: classes, interfaces, enums, records, annotation
  types, sealed classes, fields, methods, constructors, type parameters and arguments, nested
  and inner classes (of generic classes too), imports, class annotations, text blocks; bodies
  and initializers are skipped as dotty skips them, a static final field of a literal keeps it
  as a constant (`tryConstant`).
- Each class is turned, on its first lookup, into the `ClassInfo` javac would write for its
  signatures (access flags, generic signatures, descriptors, inner-class entries, permitted
  subclasses, the members javac adds: default constructor, enum `values` and `valueOf`, a
  record's canonical constructor, accessors and component fields), and entered through ADR
  0107's class-file reader, which makes a class with permitted subclasses sealed, as dotty's
  `ClassfileParser` does. An inner class named alone in its generic outer class gets no outer
  type arguments, so the reader enters it as `Outer.this.Inner`, as dotty types it in a Java
  unit. Names are resolved as in a Java unit: type variables, member classes of the
  enclosing classes and their supertypes, single-type imports, the package (Scala classes of the
  run included), on-demand imports, `java.lang`.
- After the Scala classes are written, javac compiles the Java files into the same directory, with
  that directory and the classpath on its classpath.

## Consequences
- dotty enters a Java source from its `JavaParsers` trees (Namer and Typer of a Java unit), not
  from class files. The class file here is built to give the same symbols: flags, statics in the
  companion, generic signatures, varargs, enum members, annotation constructors and
  element defaults. Constants follow dotty's source rule (`tryConstant`: a literal alone, typed
  `String` or primitive), not javac's `ConstantValue`: `1 + 2` and `"a" + "b"` are read from the
  field, as dotty does.
- Differences from dotty's Java units:
  - Parameter names are `x$0`, …: named arguments to a Java source's method are refused.
  - Private members are not entered, as for class files: Scala code cannot reach them (case 1).
  - Annotations of members are dropped, as the class-file reader drops them for jars; in dotty
    they only give deprecation warnings, and `-Werror` is refused (case 1). Class annotations
    are kept: the retention of a Java annotation type of the run.
  - A negated character literal (`-'a'`) is no constant; dotty folds it as `97` (case 3,
    `docs/conformance.md`).
  - A record gets javac's private component fields and its canonical constructor only when it
    declares none of the same types; dotty adds the constructor always and no fields (case 1:
    the fields are private).
- Refused by name, as dotty's parser rejects them: receiver parameters (`formalParam` reads an
  identifier) and hexadecimal floating-point literals (`getNumber` reads no fraction in base
  16). Refused as well: names found nowhere, an output jar (`-d x.jar`), and a missing javac.
