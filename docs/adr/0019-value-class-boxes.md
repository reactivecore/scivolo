# ADR 0019 — Value class boxes without a boxed type

Status: accepted · Date: 2026-10-12

## Context
dotty's erasure types a value class's value as `ErasedValueType(V, U)` and its box as `V`, both
on the trees. The lowering keeps unerased types on trees and computes erasures on demand
(`erased_type`); a type of the box (`Type::Boxed(V)`) would have to survive every phase that
moves or retypes trees and every `match` on `Type`. Research: [13](../research/13-value-classes.md).

## Decision
- `Erased::Value(V, U)` stands for `ErasedValueType`; only `erase_value` makes it, `erase` stays
  the full erasure, which names and codegen use.
- No boxed type: `erased_type` gives the class `V` itself for constructor calls, `New`, casts
  and `this`; a cast to `V` in codegen casts to the class.
- The value inside a box is ascribed its erased underlying type, so the value left when the box
  is elided keeps that type.

## Consequences
- A tree whose box is not one of these forms (a block or conditional over boxes typed `V`)
  erases to the underlying type; Erasure adapts branches before they meet, which avoids it.
- The `Type` enum and the typer stay untouched.
