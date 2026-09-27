# Interpreter performance

State of the VM's speed, how to measure it, and the ideas not yet done, roughly in order of
expected gain per effort.

## Measuring

```sh
cargo build --release
SCALAI_LIBS=ref/libs target/release/scalai --cp tests/programs/cpu cpu      # sort + fib
SCALAI_LIBS=ref/libs target/release/scalai --cp tests/programs/fib fib 30   # calls
SCALAI_LIBS=ref/libs target/release/scalai --cp tests/programs/loops loops 100  # arithmetic loops
SCALAI_PROFILE=1 ...   # call counts per function and fallback operations, on stderr
sample <pid> 1 -file tmp/x.sample   # native profile (macOS)
```

## Where it stands (2026-09-25, M4 Pro, release)

| Program | scalai | Scala Native binary | notes |
|---|---:|---:|---|
| hello | 47 ms | 5 ms | mostly startup: loading and linking library classes |
| fib 30 | 142 ms | 7 ms | 2.7M calls, ≈15 ns per call + ≈2.5 ns per instruction |
| loops 100 | 9.3 s | 0.8 s | 10⁷-iteration sum and Collatz up to 10⁶, ≈130M loop iterations |
| cpu | 0.76 s | 22 ms | dominated by the sort's comparator chain, ~27M calls |

Steps so far and their effect on `cpu`: interpreting NIR directly 3.5 s → lowered code
with per-site caches 1.08 s → casts/boxing/vars/elem lowered 0.97 s → fused `method`+`call`
and direct frame filling 0.89 s → two-word `Flow` 0.87 s → frame-less getters, constants
and forwarders 0.80 s → virtual forwarders too 0.76 s.

## Ideas

1. **Inline small callees at lowering time.** Library code is written for Scala Native's
   whole-program optimizer (Interflow), which inlines its many tiny methods. For a static
   call or an `Invoke` whose site has seen one class, splice the callee's lowered
   instructions into the caller with renamed slots, guarded by a class check. The
   comparator chain in `cpu` is 10 calls per comparison. Expected 2–3× on collection-heavy
   code. Needs care with unwind handlers and recursion limits.
2. **Superinstructions for common shapes**, chosen with `SCALAI_PROFILE` instruction-pair
   counts, e.g. compare-and-branch (`Comp` + `If`), `Bin` on I32 with an immediate operand,
   `FieldLoad` of `this`. Removes one dispatch and one slot write each. Expected 20–40% on
   loops.
3. **Operand specialization.** `arg()` matches three operand kinds on every read. Emit
   variants per operand kind (slot/slot, slot/imm), or convert all constants into
   preloaded frame slots so every operand is a slot index.
4. **Closure compilation** (research report 03): each instruction becomes a boxed closure
   that captures resolved operands, and the dispatch `match` goes away. Reported gains are
   1.3–2× over a match-based loop. Larger rewrite; combine with 2 and 3.
5. **Cheaper frames.** Frames live on one contiguous frame stack (done with the collector);
   still to do: skip zero-filling, since SSA defines every slot before use, and avoid the
   `Rc<Code>` clone per call. Each instruction that may call stores its index in the frame
   header for stack traces.
6. **Unwind handling off the hot path.** `compute` returns `Flow<()>` for every instruction;
   separate throwing from non-throwing instructions so the common path needs no result check.
7. **Baseline JIT with Cranelift** (roadmap phase 2): translate hot lowered methods to machine
   code. Largest gain (10×+ on loops) and largest effort. Only worth it after 1–4 show their
   limits.
8. **Startup:** a prebuilt library image (roadmap phase 2). Today hello spends most of its
   47 ms opening jars, decoding NIR files and linking about 200 classes; an mmap-able index
   plus lazily decoded method bodies should bring it under 20 ms. A pre-initialized heap
   snapshot is the next step after that.
9. **Allocation:** boxing in generic collections allocates an `Integer` per element. Cache
   small boxed values the way `Integer.valueOf` does on the JVM (javalib may already do it;
   check).
10. **Exceptions:** every new `Throwable` snapshots up to 1,024 frames through the unwind
   externs (one native call per frame) and builds `StackTraceElement`s lazily. Code that throws
   in loops pays for it; a native `currentRawStackTrace` returning the snapshot as one array
   would cut the calls.
