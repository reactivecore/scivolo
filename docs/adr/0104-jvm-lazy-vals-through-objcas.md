# ADR 0104 — JVM lazy values through Scala Native's `objCAS` trees

Status: accepted · Date: 2026-11-06

## Context
dotty's `LazyVals` on the JVM gives each thread-safe lazy value a static `VarHandle` field
`x$lzyN$lzyHandle`, set in `<clinit>` by `MethodHandles.lookup().findVarHandle(...)`, and calls
the signature-polymorphic `VarHandle.compareAndSet`. Scala Native's `AdaptLazyVals`, which the
lowering ports, calls `LazyVals.objCAS(this.x$lzyN, expected, value)` instead. javalib, whose
TASTy the frontend types against, has no `MethodHandles` and no `compareAndSet`.

## Decision
- The phases run with a `Backend` (`Native`, `Jvm`) in `Lower`; `compile_units` and `lower_trees`
  take it with the generator.
- On the JVM, `LazyVals` adds the static `VarHandle` field per lazy value field (no initializer)
  and keeps the `objCAS` trees. The generator sets the handles first in the class's `<clinit>`
  and emits `objCAS(q.f, e, v)` as `getstatic f$lzyHandle; q; e; v; invokevirtual
  VarHandle.compareAndSet(Object, Object, Object)Z`.

## Consequences
- The class files are dotty's; the lowered trees are not (no `MethodHandles` calls in them).
- Once the class file reader (research 46 §7 step 10) gives JDK symbols, the trees can be
  dotty's and the generator's rule can go.
