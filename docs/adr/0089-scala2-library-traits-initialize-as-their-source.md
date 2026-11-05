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
  files importing ``language.`2.13` ``) initializes by `defKind`'s Scala 2 rule: a concrete
  method other than the constructor, or a class, is initialization.
- Function classes get no `$init$` call and closures of them stay closures (dotty's
  `Mixin.superCallOpt` and `ExpandSAMs` exempt `isFunctionClass`).

## Consequences
- Other runs, user programs and the other Scala Native libraries among them, keep
  `TreeUnpickler`'s answer.
- A library trait that now initializes gets its trait fields and super accessors as a unit trait
  does, and its SAM closures become classes, as in Scala Native's build.
