# ADR 0114 — Scoped context state stays a field of the typer

Status: accepted · Date: 2026-11-10

## Context
Some state is not bounded by one computation (ADR 0113): the GADT bounds of the cases being
typed (`Gadt`), the not-null facts of the flow typing (`Nullables`), the open implicit searches
(`History`). It is set on entering a scope and undone on leaving it while the typer recurses
through that scope. dotty keeps it in the `Context`.

## Decision
- Such state is one field of `Typer`, with its methods an `impl` of its own struct; the services
  they need (the `Universe`, `TypeOps`, the scope levels) come in as arguments.
- `Typer` keeps only the methods that need the typer: running it inside the scope
  (`in_gadt_case`, `outside_gadt_cases`) or rewriting typed trees (`gadt_cast`, `gadt_adapted`,
  `gadt_healed`).
- A component working on the state (`Narrowing`) reaches it through the typer it is passed.

## Consequences
- `Gadt` also owns the owners of polymorphic function values, whose type parameters a case
  inside them may narrow.
- A method of the state cannot take `&mut Typer`; one that needs the typer is a method of a
  component or of `Typer`.
