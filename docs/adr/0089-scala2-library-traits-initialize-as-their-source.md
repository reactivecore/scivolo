# ADR 0089 — Classes of the Scala 2 library initialize as their source while it is built

Status: accepted · Date: 2026-11-05

## Context
dotty decides a class's `NoInits` from its source (`TreeInfo.defKind`) or its TASTy
(`TreeUnpickler.indexStats`). In a unit importing ``language.`2.13` `` (`shouldBehaveAsScala2`),
a concrete method or a nested class also counts as initialization; TASTy has no such rule. Scala
Native builds scalalib from the whole library's sources in one run, so a mixin such as `Product`
or `Stepper` is source there, gets an `$init$`, and every class mixing it in calls it. We compile
one scalalib file at a time against the published `scala-library` TASTy, where these traits are
`NoInits`. A user program compiled by dotty against the same TASTy calls none of them.

## Decision
- A run builds the Scala library when a unit replaces a top-level TASTy class of package `scala`
  outside `scala.scalanative` (the namer's `unenter_classpath_members`).
- In such a run, a class of a TASTy file with the Scala 2 library attribute (written for the
  files importing ``language.`2.13` ``) initializes by `defKind`'s Scala 2 rule: a lazy value,
  a concrete method other than the constructor, or a class, is initialization.
- `Some`, `Tuple1` to `Tuple22` and their objects initialize nothing in any run, wherever dotty
  asks `isNoInitsRealClass` (dotty's `isAssuredNoInits`).
- Function classes get no `$init$` call (dotty's `Mixin.superCallOpt` exempts
  `isFunctionClass`) and closures of them stay closures (`ExpandSAMs` leaves function literals).

## Consequences
- Other runs, user programs and the other Scala Native libraries among them, keep
  `TreeUnpickler`'s answer. auxlib replaces only Java classes of the library and is no such run.
- A program replacing a class of the library counts as building it; the traits that then
  initialize have an empty `$init$`, which the published scalalib defines.
- A library trait that now initializes gets its trait fields and super accessors as a unit trait
  does, and its SAM closures become classes, as in Scala Native's build.
