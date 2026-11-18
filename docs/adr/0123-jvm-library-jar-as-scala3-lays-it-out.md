# ADR 0123 — The JVM library jar takes from Scala 2.13 what scala3's build takes

Status: accepted · Date: 2026-11-15

## Context
#222's second criterion runs dotty's tests/run against a library jar built from our class files.
The published `scala-library-3.9.0.jar` is not dotty's output alone (ADR 0110): scala3's sbt
build (`project/ScalaLibraryPlugin.scala`) replaces the classes of 49 names in
`ScalaLibraryFilesToCopy.filesToCopy` (`Function1`, `Tuple2`, `ArrayOps`, `Range`, …, each with its
nested `NAME$…` classes) by Scala 2.13's, adds 2.13's classes dotty does not generate
(`Function1$mcII$sp`), and `StripScala2Annotations.patchFile` gives each replaced top-level class
the `TASTY` attribute (the UUID) of dotty's TASTy of the same name. The `.java` sources are
compiled by javac. `just jvm-lib-compile` compiles each `.scala` file alone.

## Decision
- `just jvm-lib-jar scalalib` packs the last full `jvm-lib-compile` run, entry by entry:
  1. ours: the class files and TASTy each compiled file wrote;
  2. a refused file's class files and TASTy from the oracle (dotty without `-opt`), counted;
  3. the published jar's classes of `filesToCopy` and its classes the oracle lacks, their
     `TASTY` attribute set to our class's of the same name;
  4. the published jar's classes of `.java` sources and its resources.
- All TASTy files are ours (or the oracle's, for a refused file); none come from the published
  jar.
- `libs/jar.tsv` lists each entry's origin; the counts are printed and head the report of a run
  on the jar.
- The build stops unless the jar has the published jar's entries and each class's `TASTY`
  attribute is the UUID of its TASTy file.
- `just dotty-jvm-report "" run tmp/lib-jvm/scalalib/libs` types the last group against our jar
  and runs it on it; earlier groups stay compiled by scala-cli against the published jar. Given
  the jar itself, the last group is typed against the published jar and run on ours, which tells
  differences of our class files from those of our TASTy.
- **Not taken:** a jar of our class files alone, in which a test could differ from the published
  jar through the classes scala3's build takes from Scala 2.13.

## Consequences
- The jar is equivalent to the published one in which classes are dotty's and which 2.13's, so
  a test that differs between the two names our class files or TASTy, not the layout.
- A `TASTY` attribute in only one of 2.13's class and ours stops the build, as `patchFile` does.
- The jar is not ours alone: the counts say how much is dotty's, Scala 2.13's and javac's.
