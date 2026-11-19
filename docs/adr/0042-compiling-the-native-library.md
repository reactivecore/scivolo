# ADR 0042 — Compiling the Scala Native library: artifacts and C parts

Status: accepted 2026-11-18 · Date: 2026-10-24 · Research: [36](../research/36-compiling-the-native-stdlib.md)

## Context
Issue #168 asks scivolo to compile the Scala Native standard library (nativelib, clib, posixlib,
windowslib, javalib, auxlib, scalalib) as nscplugin and the sbt build do, and to produce the same
artifacts. The published `_native0.5_3` jars hold NIR, the C/C++/assembly sources as resources
under `scala-native/`, and, for the four interop libraries, `.class` and `.tasty` files. Scala
Native never compiles the C sources in its build; its toolchain compiles them with clang when a
program is linked. scivolo compiles a curated copy (`native/`) with clang in `crates/snrt`.

## Decision
- **C parts.** The C sources stay what they are upstream: resources, copied into a compiled
  module's jar unchanged. scivolo compiles them with the system clang once per workspace build
  (`crates/snrt/build.rs`), never at run time, and ships no C compiler. `native/` remains the
  curated copy of the files the VM and native executables need; `just native-stale`, part of
  `just ci`, lists its differences from the jars' resources, the patched and glue files excepted.
- **The same artifacts** means, per module: every `.nir` file identical to nscplugin's
  (positions relative to the source directory, as upstream), the `scala-native/` resources
  copied verbatim, and the manifest. Accepted NIR differences are the order of var slots, which
  Scala Native takes from a hash set (`var-order`), and those `docs/conformance.md` names as case
  1 or 3 of `CLAUDE.md`. `.class` files are not produced for now; `scivolo-jvm` could produce them
  if JVM-side tooling on these jars becomes a goal. TASTy comes from our pickler
  (`scivolo-lower --pickle`) in the jars whose published ones carry TASTy; javalib's, which its
  published jar lacks, goes into a TASTy-only jar as `libs/javalib-tasty-0.5.12.jar`.
- **Stages.** Per-file compilation against the module's own published TASTy first (the measuring
  tool), then a module as one unit against its dependencies' jars, then scalalib from the patched
  3.9.0 sources, then TASTy.
- **Oracle.** Scala Native's own build of the same version (`just lib-oracle`, sbt), compared
  with `scivolo-nir-dump --debug --sorted`, as `just nir-cmp` compares test programs. The
  published jars are compiled with Scala 3.1.3 (`scala3PublishVersion`), whose lazy values and
  pattern matches differ from 3.9's; the build compiles with 3.9.0 and the published compiler
  plugin of that version (ADR 0046).

## Consequences
- The library build needs `ref/scala-native` (`just fetch-scala-native`) and clang, nothing else;
  no sbt, no JVM, except for the TASTy of a module until stage 4 and for the oracle.
- A compiled module whose NIR differs from upstream is a frontend bug or a named accepted
  difference, never a silently different library.
- Without `.class` files the jars are unusable for JVM-side tooling (Scaladoc, IDEs); scivolo's jars are
  for scivolo and the Scala Native toolchain.
