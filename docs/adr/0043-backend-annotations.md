# ADR 0043 — A backend's annotations belong to its code generator

Status: accepted 2026-11-18 · Date: 2026-10-24 · Research: [36](../research/36-compiling-the-native-stdlib.md) §3 G2

## Context
Scala Native's library annotates definitions with `@extern`, `@link`, `@alwaysinline`, `@stub`
and others that only its code generator reads (nscplugin's `genClassAttrs`, `genMethodAttrs`).
The typer refused 18 of them by name, so Native knowledge sat in the frontend. A JVM or JS
backend may be added later, with annotations of its own (`@JSExport`, `@native`, …).

## Decision
- The typer stays backend-neutral: it resolves every annotation's class and records it on the
  symbol (`Universe::annotate`), with the string literal of a one-argument annotation
  (`Universe::annotate_string`). Annotations whose meaning dotty's own phases implement are
  handled there, independent of the backend: the typer adds the accessors of `@BeanProperty`, the
  lowering refuses a reference to a `@compileTimeOnly` definition.
- Each backend's code generator owns the annotations it reads: what each gives, or that it is
  refused. Scala Native's are the table in `crates/lower/src/codegen/annotations.rs`; the lowering
  checks every definition of a unit against it before its phases run, so an annotation it does not
  implement is refused, never ignored. The JVM backend's are in `crates/jvm/src/codegen`
  (`annotation_holders.rs`, `annotation_values.rs`, `java_annotations.rs`).
- `@static` follows the same split: the namer keeps only dotty's `CheckStatic` checks, and what a
  static member becomes is the lowering's (`phases/static_members.rs`, `static_values.rs`), as
  dotty's `MoveStatics` and Scala Native's fields of the object differ by backend.
- A new backend adds its own handling in its code generator; the typer does not change.
- **Not taken:** a table in the typer of the annotations and `@static` shapes a backend refuses,
  the earlier state, which puts each backend's limits into the frontend.

## Consequences
- `just typer-report` types files with Native annotations that it refused before; the refusal
  moves to the lowering, which every path to code (scripts, REPL, macros, `scivolo-aot`) runs.
- A backend refuses only annotations it knows and cannot implement; one that no phase or backend
  reads is ignored, as dotty ignores annotations no phase reads.
