# ADR 0110 — The JVM library compile compares javap text per class with dotty's unoptimized output

Status: accepted · Date: 2026-11-09

## Context
#222 compiles Scala's library for the JVM with `scivolo-jvm` and needs an oracle. The published
`scala-library-3.9.0.jar` is not dotty's plain output: scala3's build compiles it with
`-opt -opt-inline:**,!java.**` (the backend optimizer: inliner, closure optimizer, local
optimizations) and replaces 49 classes and their nested ones (`Function1`, `Tuple2`, `ArrayOps`,
`Range`, …) by Scala 2.13's, adding 2.13's specialized classes (`Function1$mcII$sp`)
(`project/Build.scala`, `ScalaLibraryPlugin.scala`). Our class files are not byte-identical to
dotty's (constant pool order, `StackMapTable` encoding), as `just jvm-check` already accepts for
single files.

## Decision
- The oracle is dotty 3.9.0 (`scala3-compiler_3`, through coursier) compiling all `.scala` and
  `.java` files of the library's sources jar in one run with the library build's options without
  `-opt`, once, into `tmp/lib-jvm/scalalib/dotty`.
- `just jvm-lib-compile scalalib` compiles each `.scala` file on its own with
  `scivolo-jvm --scalalib` (the library's classes compiled in the same run) and the same options,
  but `-encoding UTF8` and `--java-output-version:17`, which are its only settings.
- Each class file is compared with the oracle's as `just jvm-check` compares them, through
  `scripts/jvm_check.py`: first `javap -c` text (code), then `javap -v` text (flags, attributes,
  line numbers); a class is `same`, `code differs` or `attributes differ`, never by bytes.
- An oracle class belongs to the source its `SourceFile` attribute names in its package
  directory; a class of no such source is `oracle-only`, never `same`.
- A timeout is run once more before it counts.
- TASTy files are not compared yet; a file whose TASTy cannot be pickled is refused.

## Consequences
- Constant pool order and frame encoding never count as a difference; javap's text of an
  instruction does (`ldc` against `ldc_w`).
- The optimizer and the Scala 2.13 classes of the published jar are no difference.
- A file compiled alone does not see the classes of the other files as sources: their TASTy comes
  from the published jar.
