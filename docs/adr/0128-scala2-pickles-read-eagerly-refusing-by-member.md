# ADR 0128 — Scala 2 pickles of jars are read eagerly, per class file, refusing by member

Status: accepted · Date: 2026-11-20 · Amends ADR 0107

## Context
Libraries published for Scala 2.13 (#272, Scala.js' library for #170) carry their signatures as
Scala 2 pickles: the `bytes` of a `@ScalaSignature` (or `@ScalaLongSignature`) annotation of the
class file with a `ScalaSig` attribute. ADR 0107 refused every class file marked by Scala.
dotty reads the pickles with `Scala2Unpickler`, lazily (a completer per symbol) and with trees for
annotation arguments.

## Decision
- **Marks** (dotty's `unpickleOrParseInnerClasses`): a `TASTY` attribute is refused (a Scala 3
  class file without its TASTy file); a `Scala` attribute alone is a compilation artifact, absent
  as dotty's `NoEmbedded` (an object's class `C$`, `$sp` classes); `ScalaSig` is a pickle. Only
  `p/C.class` (or an object's mirror class, `p/package.class` for a package object) holds it, for
  the class, its object and nested ones.
- **Eager**: the first lookup of `C` in package `p` (or of a name `p` lacks, for its package
  object) enters the class, its object's class `C$` and its object as ADR 0107 lays out a Java
  class, before anything else of the pickle; then every symbol entry, the classes' parents, the
  type members, the other members, the symbols' annotations: no completers.
- **Entries read**: names, all symbol entries, `EXTref`, `EXTMODCLASSref`, `SYMANNOT` (the
  annotation's class, its arguments left out); types `NOPREFIXtpe`, `THIStpe`, `SINGLEtpe` of
  objects and packages, `TYPEREFtpe`, `TYPEBOUNDStpe`, `REFINEDtpe` with type members,
  `CONSTANTtpe` with value literals, `CLASSINFOtpe`, `METHODtpe`, `IMPLICITMETHODtpe`, `POLYtpe`
  (methods, and type lambdas over the variances the parameters declare), `EXISTENTIALtpe` as
  `elimExistentials` eliminates it, `ANNOTATEDtpe`; versions 5.0 to 5.2.
- **Symbols and types as dotty makes them**: a getter is a value (`FIELDaccessor`, mutable without
  `STABLE`), `$init$` is a trait's constructor, a constructor is polymorphic in its class's type
  parameters; a parameter with a default is `HASDEFAULT` and its getter `m$default$N` a member;
  `<byname>[T]` is `=> T`; classes and objects of objects are entered as nested ones; a type
  member of a class is selected from its `this`, one of a refinement by name. Private members,
  case accessors `x$access$N`, extension methods of objects, existential and stray type parameters
  are skipped. Classes are Scala 2's (`Universe::set_scala2`), never `is_java_defined`, so their
  members erase as `Scala2Erasure` says. A class other than a trait initializes (dotty sets
  `NoInits` on Scala 2 traits without `$init$` only), so `new C(..)` is no pure expression.
- **Value classes** are read as Scala 3's and go through the same lowering: calls of their
  methods become `C.m$extension(u, ..)` on the object (dotty's `ExtensionMethods` adds none for
  `Scala2x`, the object's class file has them), boxing, unboxing, arrays, generic underlying
  values, universal traits and implicit value classes as dotty erases them (`TypeErasure` with
  `isScala2` differs only in `isGenericArrayElement`, already ported). The private getter of a
  value class's parameter (`private val x`) is entered private, with its class file's expanded
  name `p$C$$x` as run-time name (dotty's `ExpandPrivate`), and called virtually.
- **Refusal granularity**: a member whose type is not read is not entered and its name is
  poisoned in its class: a lookup of it counts as a refused lookup naming the member and the
  construct. A class is refused whole when its type parameters or parents are not read, or when
  an implicit member is refused. A pickle entered while another one was and
  that refers to a symbol of it is refused with it when that one is refused, or when the member it
  refers to is (eager entering of cyclic pickles binds symbols before their types are read).
- **Refused by name**, until ported: super types, path-dependent and dependent method types,
  a structural (term) member of a refinement, a refinement referring to its `this`, an existential
  dotty cannot eliminate (`FailureToEliminateExistential`), classes and objects of classes,
  expanded names and super accessors, macros, the extractor (`unapply`) of a case class (also of
  a value class); extending a class as the next section says. The extractor needs lowering.

## Differences from dotty
- An unapplied class type constructor (`Functor[List]`) stays the class reference; dotty
  eta-expands it to `[X] =>> List[X]`. The two are equivalent, and the reference is what dotty's
  TASTy holds (case 2).
- A repeated parameter type `<repeated>[T]` is read as `Seq[T] @Repeated`, the form of TASTy and
  of the typer's other repeated parameters (case 2).
- Annotation arguments are left out, and parameters are not annotated: the arguments give
  dotty's warnings and messages (`@deprecated`, `@implicitNotFound`), which the typer emits for no
  class, and `@deprecatedName`, whose old names are not found, a refusal (case 1). The children
  of sealed classes (`CHILDREN`) are not read: they give dotty's exhaustivity warnings, which the
  typer emits for no class, and the typer refuses a sum mirror of a sealed class without TASTy
  (case 1). Under `-Werror` dotty fails where this compiler does not.

## Decision: linking and extending
- **Linking guard**: the class file stays the check, not the source. A call of a member of a
  Scala 2 class, and every method of a Scala 2 base class of a class compiled, must have its
  name and erased descriptor in the class file read (the class's, or the object's `C$`), else
  the program is refused naming member and descriptor; so is the call of a value class's
  extension method, an unboxing getter and a constructor. An erasure difference becomes a refusal,
  never a `NoSuchMethodError` or a missing bridge. Fields of Scala 2 classes are never accessed.
- **Extending** a Scala 2 class or trait is allowed, as dotty's `Mixin` does it for `Scala2x`:
  a forwarder for each concrete trait method, the `$init$` call of a trait with one (a pickle
  without `$init$` is `NoInits`), a field, getter and trait setter `T$_setter_$x_$eq` for each
  concrete `val` (a pickle has no trait setters), a setter for a `var`, a lazy value calling
  `T.x$`. Refused, naming the class: one with a member not read, and one whose class file has an
  abstract method its pickle does not explain (getters of private values, super accessors).

## Alternatives
- Lazy completers as dotty: rejected, `Universe` has no completer machinery and components run
  eagerly (ADR 0113).
- Refusing the whole class on any member not read: rejected, too coarse for real jars.
- Dropping members not read: rejected, it changes overload resolution and implicit search
  silently.
- Refusing every pickle entered while a refused one was: rejected, cyclic libraries (cats) lose
  most classes to one refused class.
- Entering the private getter of a value class public under its expanded name: rejected, the
  typer would accept `c.p$C$$x`, which dotty's typer does not see.
- Taking the descriptors of the class file as for Java classes: rejected as the source of truth,
  members and methods do not map one to one (getters, expanded names, `$extension`).
- Reading trees (`readTree`) for annotation arguments: deferred.

## Consequences
- A refusal stays a refusal, never a different type: differences in meaning are refusals only.
- `just scala2-report JAR...` counts the classes read and the refusals by reason.
- Scala.js' pseudo-union `js.|` and its erasure (#170) are later work, as are the constructs
  refused above.
