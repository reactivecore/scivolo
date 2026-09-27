# ADR 0006 — Tier-up with speculation and inlining

Status: accepted · Date: 2026-09-27

## Context
Library code is written for Scala Native's whole-program optimizer (Interflow), which inlines
its many tiny methods; `cpu`'s sort makes about ten calls per comparison. At 0.76 s `cpu` was
over the phase 1 target of 0.34 s. Inlining every method at first lowering doubled the time
to lower and pushed hello past 20 ms.

## Decision
- **First lowering stays plain.** Methods are lowered as they are, with call-site caches.
- **Tier-up.** After `TIER_UP` (1,000) calls a method is lowered again from its NIR
  (`interp/speculate.rs`):
  - each virtual call whose site cache resolved to one method is split into a guarded
    direct call (`method == target`) and the original virtual call;
  - calls on module, final or exactly typed receivers become direct (`interp/devirtualize.rs`);
  - direct calls of callees with at most 24 instructions, without `Stackalloc`, `Var` or
    `Dynmethod`, are inlined for up to six rounds while the caller stays under 2,000
    instructions; a method may be inlined into itself once (`interp/inline.rs`).
- The replaced code is kept as `retired`, since running frames still execute it; frame
  headers and stack-trace addresses carry the tier to find the right code's positions.
- `SCALAI_NO_INLINE=1` turns tier-up off.

## Consequences
- `cpu` takes 0.35 s (from 0.76), fib 30 85 ms (from 142); hello is unchanged at 20 ms.
- Inlined callees have no frame of their own: stack traces show the caller's line only.
- A guard failure costs one comparison and the ordinary virtual call; there is no
  deoptimization, so speculation is always safe.
