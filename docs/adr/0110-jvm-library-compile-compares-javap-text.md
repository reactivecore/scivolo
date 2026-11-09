# ADR 0110 — The JVM library compile compares javap text per class

Status: accepted · Date: 2026-11-09

## Context
#222 compiles Scala's library for the JVM with `scivolo-jvm` and needs an oracle: the published
`scala-library-3.9.0.jar`. Our class files are not byte-identical to scalac's (constant pool
order, `StackMapTable` encoding), as `just jvm-check` already accepts for single files. Scala
Native's library compile (`just lib-compile`) compares NIR files instead.

## Decision
- `just jvm-lib-compile scalalib` compiles each `.scala` file of the library's sources jar,
  unpatched, on its own with `scivolo-jvm --scalalib` (the jar's classes compiled in the same run)
  and the library's options (`-Yexplicit-nulls`, `-language:implicitConversions`).
- Each class file is compared with the jar's as `just jvm-check` compares them, through
  `scripts/jvm_check.py`: first `javap -c` text (code), then `javap -v` text (flags, attributes,
  line numbers); a class is `same`, `code differs` or `attributes differ`, never by bytes.
- A class of the jar belongs to the source its `SourceFile` attribute names in its package
  directory. Classes of no Scala file (the Java sources) and the specialized interfaces
  (`Function1$mcII$sp`, which the library build adds) are `oracle-only`, never `same`.
- A timeout is run once more before it counts.
- TASTy files are not compared yet; a file whose TASTy cannot be pickled is refused.

## Consequences
- Constant pool order and frame encoding never count as a difference; javap's text of an
  instruction does (`ldc` against `ldc_w`).
- A file compiled alone does not see the classes of the other files as sources: their TASTy comes
  from the jar.
