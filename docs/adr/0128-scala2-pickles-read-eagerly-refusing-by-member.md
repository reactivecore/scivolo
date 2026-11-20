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
  `p/C.class` (or an object's mirror class) holds it, for the class, its object and nested ones.
- **Eager**: the first lookup of `C` in package `p` enters the class, its object's class `C$`
  and its object as ADR 0107 lays out a Java class, then reads every symbol entry, then the types:
  no completers.
- **Entries read**: names, `NONEsym`, `CLASSsym`, `MODULEsym`, `VALsym`, class type parameters,
  `EXTref`, `EXTMODCLASSref`; types `NOPREFIXtpe`, `THIStpe`, `SINGLEtpe` of objects and
  packages, `TYPEREFtpe` (classes, aliases followed, type parameters, applied), `TYPEBOUNDStpe`,
  `CONSTANTtpe` with value literals, `CLASSINFOtpe`, `METHODtpe`, `IMPLICITMETHODtpe`, `POLYtpe`;
  versions 5.0 to 5.2.
- **Symbols as dotty makes them**: a getter is a value (`FIELDaccessor`, mutable without
  `STABLE`), `$init$` is a trait's constructor, a constructor is polymorphic in its class's type
  parameters; private members, case accessors `x$access$N`, extension methods of objects and stray
  type parameters are skipped. Classes are Scala 2's (`Universe::set_scala2`), never
  `is_java_defined`, so their members erase as `Scala2Erasure` says.
- **Refusal granularity**: a member whose type is not read is not entered and its name is
  poisoned in its class: a lookup of it counts as a refused lookup naming the member and the
  construct. A class is refused whole when its type parameters or parents are not read, when it
  is a case class or object or a value class, or when an implicit member is refused.
- **Refused by name**, until ported: refinement, existential, annotated and super types, path
  dependent and dependent method types, by-name and repeated parameters, nested classes and
  objects, type members, default arguments and their getters, expanded names, macros, package
  objects, extending a class of a pickle.

## Alternatives
- Lazy completers as dotty: rejected, `Universe` has no completer machinery and components run
  eagerly (ADR 0113).
- Refusing the whole class on any member not read: rejected, too coarse for real jars.
- Dropping members not read: rejected, it changes overload resolution and implicit search
  silently.
- Taking the descriptors of the class file as for Java classes: rejected as the source of truth,
  members and methods do not map one to one (getters, expanded names, `$extension`).
- Reading trees (`readTree`) for annotation arguments: deferred.

## Consequences
- A refusal stays a refusal, never a different type: differences from dotty are refusals only.
- Scala.js' pseudo-union `js.|` and its erasure (#170) are later work, as are the constructs
  refused above.
