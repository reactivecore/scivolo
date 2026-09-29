# ADR 0009 — Library fixes in Scala: the overlay

Status: accepted · Date: 2026-09-29

## Context
The VM fixes javalib bugs and adds JDK behaviour (ADR 0008). Written as Rust natives, such fixes
are opaque to the inliner, build objects and arrays by hand, reach the original method through
a side door, and cannot be offered upstream as they are. The published javalib holds NIR only:
no class files or TASTy to compile against, so code using its internals cannot be compiled
against it directly.

## Decision
- **Overlay:** Scala sources in `overlay/src`, compiled by `just overlay` with scala-cli and
  Scala Native into committed NIR in `overlay/nir`, which the VM embeds. `just overlay-stale`
  compares the committed NIR with a rebuild, as for the test programs.
- **Shadow declarations:** an overlay file declares a library class under its own name, with
  only the members it needs; the compiler sees it instead of the JDK's. A member the overlay
  uses but does not change is a stub, a method whose body only calls `???`. Helper classes go
  into package `scalai` and are taken whole.
- **Member-level precedence:** when the VM loads a library class the overlay also declares,
  each overlay method replaces the library's or is added to the class, including the ones the
  compiler generates (static forwarders, `writeReplace`), which match the library's. The
  library keeps its class definition, fields and constructors; stubs, fields and abstract
  methods of the overlay are ignored. Each stub and field must name a library member, and each
  added method must be listed in `overlay/added.txt`; otherwise the class fails to load, every
  time. A unit test loads every overlay class against the library. A Rust native for a method
  still takes precedence over its overlay definition.
- **What goes where:** library behaviour (bug fixes, JDK methods javalib leaves out or stubs)
  goes into the overlay; what needs the VM's own state (frames, threads, the classpath,
  intrinsics) stays in Rust natives. Overlay code reaches C through `@extern` declarations as
  any Scala Native code does.

## Consequences
- The javalib fixes (docs/conformance.md) and the JDK behaviour of ADR 0008 move from Rust
  natives into the overlay; its methods are inlined like library code, and a fix's diff
  against javalib's source is the upstream patch.
- Overlay code must not declare fields or constructors the library lacks, and uses the
  library's accessor methods instead of its private fields. An inner class of a shadowed class
  is declared in the companion object, which gives it the same name without an outer reference.
- A method whose name the compiler generates (lambdas, `$anonfun`) cannot be replaced directly;
  the overlay replaces the method that calls it.
