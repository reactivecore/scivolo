# ADR 0044 — `@static` checked by the typer, moved and refused by the lowering

Status: accepted · Date: 2026-10-24

## Context
`@static` members are a dotty feature whose checks (`CheckStatic`) do not depend on the
backend, while what a static member becomes does: dotty's `MoveStatics` moves them to the
companion class, and Scala Native emulates a static field as a field of the object with a
static getter and a `<clinit>` that initializes it. The typer refused `@static` values unless
initialized by a literal, a limit of the lowering, and accepted `@static` methods, which the
lowering compiled as methods of the object, so a call initialized the object where Scala
Native's does not. JVM or JS backends may follow.

## Decision
- The namer keeps only `CheckStatic`'s checks; it knows nothing of how a backend lowers a
  static member.
- The lowering, the Scala Native backend, owns the rest in `phases/static_members.rs`: it moves
  `@static` methods to the companion class as static methods (`MoveStatics`) and refuses what it
  does not lower yet: `@static` values and variables, methods that use the object's `this` or
  define local methods or classes, objects whose companion is a trait or in another unit, and
  references to `@static` members of the library.
- Another backend gets its own phase; annotations whose meaning is backend-specific are decided
  in the backend's crate, not in the typer's tables.

## Consequences
- i17332, i19396 and static/i2054 lower as Scala Native compiles them; i5924b, i5924c and
  statics stay refused, now by the lowering.
- The typer's `REFUSED` table of annotations still lists Scala Native's own annotations; moving
  it to the lowering is open.
