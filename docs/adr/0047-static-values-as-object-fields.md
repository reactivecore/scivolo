# ADR 0047 — `@static` values as fields of the object, set by the companion's `<clinit>`

Status: accepted · Date: 2026-10-26

## Context
ADR 0044 left `@static` values refused. dotty's `MoveStatics` moves them to the companion class
and initializes them in a `<clinit>` there; Scala Native's nscplugin emits a static field as a
field of the object's class with a static getter on the companion, and the `<clinit>` stores
into it through the object's instance. The lowering's phases run in another order than dotty's
(the move comes first, before `Getters` and constant folding).

## Decision
- `phases/static_values.rs` keeps each `@static` value in the object as a field without an
  initializer (flag `STATIC`), adds a static `<clinit>` to the companion storing the initial
  values in order, and moves the setters of `@static` variables there as static methods storing
  into the field.
- Codegen names the static getter of such a field on the companion class, as for enum values.
- Constant folding skips the left-hand sides of assignments, which the `<clinit>` of a `final
  val` has before `FirstTransform` runs.
- Still refused: `@static` lazy values, values of objects whose companion is a trait, a value
  class or in another unit.

## Consequences
- statics and i5924c lower as Scala Native compiles them (`tests/programs/Lw26statics`,
  `Lw26staticvals`); i5924b (a trait companion) stays refused.
