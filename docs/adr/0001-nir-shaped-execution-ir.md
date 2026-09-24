# ADR 0001 — One NIR-shaped execution IR; TASTy for types, NIR for code

Status: proposed · Date: 2026-09-24

## Context
Two compiled forms of Scala 3.9 are available without a JVM at run time:
- **TASTy**: typed, pre-erasure trees. Givens and overloads are already
  resolved. Pattern matches, mixins, lazy vals, inline calls and macros are
  still unlowered. The stdlib ships it (941 files); javalib does not.
- **NIR** (Scala Native 0.5.12): post-erasure SSA, fully lowered. Scala Native
  publishes it for the stdlib, javalib and nativelib.

Both readers exist as spikes (`spikes/01`, `spikes/04`) and decode every
library file without error. Executing TASTy means porting dotc's lowering
phases into the interpreter. Executing NIR means user code needs a compiler
that emits NIR.

## Decision
The VM executes one register/SSA IR that NIR maps onto 1:1.
- **Code** comes from NIR: precompiled libraries, and user code compiled by
  dotc + nscplugin (JVM-hosted, or scalino's native build) behind a
  content-hash cache.
- **Types** come from TASTy: the future Rust frontend type-checks scripts
  against library TASTy and lowers them into the same IR itself. That
  lowering is the part of dotc's pipeline after the typer, restricted to
  script code.

## Consequences
- The Scala Native conformance suites run as soon as the VM runs NIR, with
  no frontend of our own. Their test code is already NIR.
- javalib (`java.lang.*` etc.) comes for free, along with the Scala Native
  subset and its object layout. So does its C runtime, linked through FFI.
- The own frontend has to emit exactly the globals Scala Native mangles, so
  its erasure and name mangling must agree with nscplugin. This needs a spike
  of its own.
- An NIR compat bump (Scala Native 0.6) forces a reader update.
