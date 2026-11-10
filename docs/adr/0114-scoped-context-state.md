# ADR 0114 — Scoped context state stays a field of the typer

Status: accepted · Date: 2026-11-10

## Context
ADR 0113 makes a bounded computation a component, created, run and dropped. Some state is not
bounded by one computation: the GADT bounds of the cases being typed (`Gadt`), the not-null
facts of the flow typing (`Nullables`), the open implicit searches and their budget (`History`).
It is set on entering a scope and undone on leaving it, while the typer recurses through that
scope. dotty keeps it in the `Context`.

## Decision
- Such state is one field of `Typer`, not created and dropped per computation.
- Its methods are an `impl` of its own struct, not of `Typer`; the services they need (the
  `Universe`, `TypeOps`, the scope levels) come in as arguments.
- `Typer` keeps only what must run the typer inside the scope: `in_gadt_case`,
  `outside_gadt_cases`.
- A component that works on the state (the narrowing of the GADT bounds by a pattern, `Narrowing`)
  reaches it through the typer it is passed.

## Consequences
- `Gadt` owns the type parameters narrowed by the cases being typed, the types they bind, and the
  owners of polymorphic function values, whose type parameters it may narrow too.
- `Nullables` owns the not-null facts of the contexts, the infos of typed trees and the tracked
  variables. Its methods take `Flow`, which lends them the typed trees, `TypeOps`, the definitions
  and the unit's syntax trees; it is one module directory, so its fields are private to them.
- A method of the state cannot take `&mut Typer`; one that needs a typing callback is a method of
  a component or of `Typer`.
