# ADR 0043 — A backend's annotations belong to its code generator

Status: proposed · Date: 2026-10-24 · Research: [36](../research/36-compiling-the-native-stdlib.md) §3 G2

## Context
Scala Native's library annotates definitions with `@extern`, `@link`, `@alwaysinline`, `@stub`
and others that only its code generator reads (nscplugin's `genClassAttrs`, `genMethodAttrs`).
The typer refused 18 of them by name, so Native knowledge sat in the frontend. A JVM or JS
backend may be added later, with annotations of its own (`@JSExport`, `@native`, …).

## Decision
- The typer stays backend-neutral: it resolves every annotation's class and records it on the
  symbol (`Universe::annotate`), with the string literal of a one-argument annotation
  (`Universe::annotate_string`). It refuses only annotations whose meaning dotty's own phases
  implement (`@compileTimeOnly`, `@BeanProperty`, …).
- Each backend owns one table of the annotation classes its code generator reads: the attribute
  each gives, or refused. Scala Native's is `crates/lower/src/codegen/annotations.rs`;
  the lowering checks every definition of a unit against it before its phases run, so an
  annotation it does not implement is refused, never ignored.
- A new backend adds its own table and check in its code generator; the typer does not change.

## Consequences
- `just typer-report` types files with Native annotations that it refused before; the refusal
  moves to the lowering, which every path to code (scripts, REPL, macros, `scivolo-aot`) runs.
- An annotation is refused only by a backend that lists it; one that no table lists is ignored,
  as dotty ignores annotations no phase reads.
