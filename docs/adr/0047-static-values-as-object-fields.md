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
- The `<clinit>` is positioned at the first initial value, or at the companion's first
  statement if before it and the companion has lazy values (dotty's `LazyVals` offset fields
  come first); refused for such a companion with constructor parameters, parents but `Object`
  or synthetic objects, where the offsets' position was not checked.
- Codegen names the static getter of such a field on the companion class, as for enum values.
- Constant folding skips the left-hand sides of assignments, which the `<clinit>` of a `final
  val` has before `FirstTransform` runs.
- Still refused: `@static` lazy values, values of objects whose companion is a value class or in
  another unit. A trait companion takes them like a class (#184), its classes get no forwarders
  to them; the VM links a trait on the use of a member, which runs its `<clinit>`.

## Consequences
- statics and i5924c lower as Scala Native compiles them (`tests/programs/Lw26statics`,
  `Lw26staticvals`), and so does i5924b (`Lw30statictrait`).
- The VM runs a `<clinit>` on first use of its class, Scala Native all of them at startup; a
  program can observe the difference (`docs/conformance.md`).
