# ADR 0107 — On the JVM, what the library lacks of Java is entered from class files

Status: accepted · Date: 2026-11-08

## Context
The typer knows the JDK through javalib's TASTy and the JDK declarations (ADR 0072). javalib lacks
whole packages (`java.time`, `java.text`), classes (`ConcurrentSkipListMap`), members of classes it
has (`Class.getDeclaredMethods`, `EnumSet.allOf`) and annotation classes (`@FunctionalInterface`,
used by 87 files of the Scala library). Jars hold Java classes too (`scala.runtime.BoxesRunTime`).
dotty reads all of them from class files (`ClassfileParser`). ADR 0106 finds the JDK's.

## Decision
- A universe has a JDK and reads class files only for the JVM backend (`Universe::set_jdk`,
  `use_java_classfiles`); the NIR path does neither.
- **JDK classes.** A name a `java.*`, `javax.*` or `sun.*` package (or the root, for `java`,
  `javax`, `sun`) lacks in the library, its files and its package objects is looked up in the JDK
  last: a JDK package is entered, a public top-level class with its object and object's class.
- **JDK members.** With a JDK, the first failed lookup of a name in a `java.*` or `javax.*` class
  of the library other than `Object` enters, once per class and base classes first, the public
  and protected members of its class file named as no library member of the class or its bases
  is: instance members into the class, static ones into its object's class. The JDK class's type
  parameters stand for the library class's by position; a different count enters no instance
  member. A name of which one member is refused is not entered, so no overload set is partial.
- **Classes of jars.** A Java class file of a jar on the JVM's class path (on `--cp`, or the Scala
  library's own) is entered on lookup if the package lacks the class and its object; packages of
  such files are entered when the universe is opened. The library's runtime stubs of these classes
  (`Statics`, `BoxedUnit`, the `*Ref` classes) give way to them. The Native backend keeps the stubs.
- **Nested classes** of `InnerClasses` are entered with their class, static ones into its object's
  class, inner ones into it (`enterOwnInnerClasses`). An inner class's constructor drops the outer
  instance its descriptor starts with, and its signatures see the enclosing classes' type
  parameters (`normalizeConstructorParams`, `addEnclosingTParams`). A public or protected nested
  class a library class lacks is entered on lookup (`Runtime.Version`).
- **Types.** A class's type parameters and parents come from its `Signature`, else from its
  superclass and interfaces; a member's type from its `Signature`, else its descriptor, as dotty's
  `sigToType` and `cook` read them (`Object` as `<FromJavaObject>`, a raw type applied to
  wildcards, varargs `Array[T] @Repeated`, a constructor polymorphic in the class's type
  parameters). A method's erasure is its descriptor's. A field's `ConstantValue` is its constant
  type. An enum with constants, or a class with permitted subclasses, is sealed.
- **Visibility** is what dotty gives (`getPrivateWithin`): `private`, `protected[p]`, or
  `private[p]` if package-private, `p` the class's package.
- **Annotation classes** are entered with dotty's constructor (`addAnnotationConstructor`): a
  parameter per element. The class file's `@Retention` and `@Repeatable` are read. As a Java
  interface it is a trait: `new` of it is an error (`… is a trait; it cannot be instantiated`) but
  inside an annotation of a Java annotation class, its arguments included (dotty's
  `PostTyper.inJavaAnnot`).
- **Not entered**, so refused as not found: synthetic and bridge members, private ones but
  constructors, native varargs methods outside `MethodHandle` and `VarHandle`, constructors with
  type parameters of their own, a class of a Scala 2 class file or of a Scala 3 one without its
  TASTy file (`Scala`, `ScalaSig` or `TASTY` attribute), a top-level class whose name holds a `$`.
- **Not taken:** declaring what javalib lacks by hand, as ADR 0072 declares what it has.

## Consequences
- The library's classes stay; which of their members count is ADR 0118's.
- The NIR path, without a JDK, is unchanged.
- The `Exceptions` attribute and deprecation are not read from class files, so deprecation warnings
  of such members are missing.
