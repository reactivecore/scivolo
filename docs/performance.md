# Interpreter performance

State of the VM's speed, how to measure it, and the ideas not yet done, roughly in order of
expected gain per effort.

## Measuring

```sh
just bench cpu                     # wall time of tests/programs/cpu (sort + fib)
just bench-base OLD/scalai cpu     # alternating runs of an older build and the current one
just vm-profile cpu                # call and instruction counts; `just script-profile FILE`
python3 scripts/profile.py cpu     # native profile (macOS `sample`)
```

## Where it stands (2026-09-28, M4 Pro, release)

| Program | scalai | Scala Native binary | notes |
|---|---:|---:|---|
| hello | 10 ms | 5 ms | runtime initialization |
| moderate | 19 ms | 5 ms | |
| fib 30 | 63 ms | 7 ms | 2.7M calls; the recursion is inlined one level |
| loops 100 | 3.8 s | 0.8 s | 10⁷-iteration sum and Collatz up to 10⁶, ≈130M loop iterations |
| cpu | 0.32 s | 22 ms | fib 30 and a sort through a boxed comparator chain |

Steps so far and their effect on `cpu`: interpreting NIR directly 3.5 s → lowered code
with per-site caches 1.08 s → casts/boxing/vars/elem lowered 0.97 s → fused `method`+`call`
and direct frame filling 0.89 s → two-word `Flow` 0.87 s → frame-less getters, constants
and forwarders 0.80 s → virtual forwarders too 0.76 s → dense slots, copy aliases and
module constants → compare-and-branch fusion and sized memory access → tier-up with
devirtualization, guarded speculation of virtual calls and inlining (ADR 0006) 0.35 s →
speculation inside inlined callees 0.34 s → constant conversions folded, var values written
in place, jumps to a branch replaced by the branch 0.32 s (`loops` 7.0 → 3.8 s).

Generic `==` (`BoxesRunTime.equals`) is never inlined; its call compares two references
or two `java.lang` boxes of one class directly (`Instr::BoxedEquals`), which made building a
`ListSet` of boxed `Int`s 3× faster.

## Ideas

1. **Deeper inlining** (ADR 0006 does small callees after tier-up): polymorphic sites with
   two targets, larger callees on hot paths.
2. **More superinstructions**, chosen with `SCALAI_PROFILE` instruction counts, e.g. `Bin` on
   I32 with an immediate operand, `FieldLoad` of `this` (compare-and-branch is done).
   Removes one dispatch and one slot write each.
3. **Operand specialization.** `arg()` matches three operand kinds on every read. Emit
   variants per operand kind (slot/slot, slot/imm), or convert all constants into
   preloaded frame slots so every operand is a slot index.
4. **Closure compilation** (research report 03): each instruction becomes a boxed closure
   that captures resolved operands, and the dispatch `match` goes away. Reported gains are
   1.3–2× over a match-based loop. Larger rewrite; combine with 2 and 3.
5. **Cheaper frames.** Frames live on one contiguous frame stack and are not zero-filled;
   still to do: avoid the `Rc<Code>` clone per call.
6. **Unwind handling off the hot path.** `compute` returns `Flow<()>` for every instruction;
   separate throwing from non-throwing instructions so the common path needs no result check.
7. **Baseline JIT with Cranelift** (roadmap phase 2): translate hot lowered methods to machine
   code. Largest gain (10×+ on loops) and largest effort. Only worth it after 1–4 show their
   limits.
8. **Startup:** the library image and lazy decoding (ADR 0005) brought hello from 47 to 20 ms;
   linking classes from their fields alone (not decoding every method) and ASCII strings
   straight from the file to 10 ms. Of hello's 10 ms, ~2 ms are process start, ~2.6 ms
   reading the indexes of the 328 files it touches, ~1.4 ms decoding the 1,093 definitions
   it uses, ~1.6 ms lowering 736 methods and ~1 ms laying out classes. Next: a prebuilt
   index or snapshot of the decoded world in the library image, then a heap snapshot.
9. **Allocation:** boxing in generic collections allocates an `Integer` per element. Cache
   small boxed values the way `Integer.valueOf` does on the JVM (javalib may already do it;
   check).
10. **Exceptions:** every new `Throwable` snapshots up to 1,024 frames through the unwind
   externs (one native call per frame) and builds `StackTraceElement`s lazily. Code that throws
   in loops pays for it; a native `currentRawStackTrace` returning the snapshot as one array
   would cut the calls.
