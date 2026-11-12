# Performance

State of the VM's speed, how to measure it, compile speed against scalac, and the ideas not yet
done, roughly in order of expected gain per effort.

## Measuring

```sh
just bench cpu                     # wall time of tests/programs/cpu (sort + fib)
just bench-base OLD/scivolo cpu     # alternating runs of an older build and the current one
just vm-profile cpu                # call and instruction counts; `just script-profile FILE`
python3 scripts/profile.py cpu     # native profile (macOS `sample`)
```

## Where it stands (2026-11-03, M4 Pro, release, minima)

| Program | scivolo | Scala Native binary | notes |
|---|---:|---:|---|
| hello | 7.9 ms | 5 ms | runtime initialization |
| moderate | 17 ms | 5 ms | |
| fib 30 | 48 ms | 7 ms | 2.7M calls; the recursion is inlined one level |
| loops 100 | 2.8 s | 0.8 s | 10⁷-iteration sum and Collatz up to 10⁶, ≈130M loop iterations |
| cpu | 0.26 s | 22 ms | fib 30 and a sort through a boxed comparator chain |

What makes it fast: lowered code with per-site caches, fused instructions, frame-less getters
and forwarders, tier-up with devirtualization, guarded speculation and inlining (ADR 0006),
and `Int`/`Long` operations resolved when lowered (`ops/int.rs`).

Measured and not pursued: virtual call sites that change target are rare (a few dozen target
changes per run of `cpu` or `moderate`, none in hot loops), so guarded inlining of a second
target would not pay; moving block-parameter moves out of `Target` into one table per method
gained nothing. The `Rc<Code>` clone per call stays: one non-atomic increment and decrement,
and it keeps the code alive while tier-up replaces it.

Generic `==` (`BoxesRunTime.equals`) is never inlined; its call compares two references
or two `java.lang` boxes of one class directly (`Instr::BoxedEquals`), which made building a
`ListSet` of boxed `Int`s 3× faster.

## Compile speed against scalac

`just bench-compile [CODEBASE] [RUNS]` compiles each codebase as one unit, with the same sources
and classpath, by `scivolo-jvm`, `scivolo-lower` and scalac 3.9.0 on a fresh JVM (cold) and in
one JVM after three compilations (warm, as a compile server). Minima of 3 runs, M4 Pro, release,
on a shared machine; CPU is user + sys, so scalac's includes its JIT and GC threads.

| codebase | files | tool | refused | wall s | CPU s | peak RSS MB |
|---|--:|---|--:|--:|--:|--:|
| tasty-core | 8 | scivolo-jvm | 0 | 0.08 | 0.08 | 42 |
| | | scivolo-lower | 0 | 0.07 | 0.07 | 38 |
| | | scalac cold | | 1.65 | 4.27 | 267 |
| | | scalac warm | | 0.22 | 0.61 | 486 |
| upickle-core 4.4.3 | 27 | scivolo-jvm | 0 | 0.23 | 0.22 | 55 |
| | | scivolo-lower | 0 | 0.19 | 0.19 | 53 |
| | | scalac cold | | 2.96 | 10.06 | 355 |
| | | scalac warm | | 0.36 | 1.36 | 667 |
| Scala's library (JVM) | 649 | scivolo-jvm, file by file | 31 | 51.4 | 51.0 | 140 |
| | | scalac cold | | 16.7 | 74.8 | 1,388 |
| | | scalac warm | | 6.4 | 10.6 | 2,163 |
| javalib (Native) | 601 | scivolo-lower | 2 | 84.9 | 84.9 | 2,174 |
| | | scalac cold | | 11.1 | 48.6 | 845 |
| | | scalac warm | | 3.3 | 5.4 | 1,541 |

- Against cold scalac, scivolo is 13–23× faster in wall time on tasty-core and upickle-core;
  against warm scalac it is 1.6–3.1× in wall time and 6.1–8.7× in CPU time (loaded machine).
  upickle-core stays below 2× in wall time against warm scalac (#248); #249 (Scala's library)
  and #250 (javalib) name the hot spots of the others.
- Scala's library row is file by file. `just bench-compile scalalib` compiles it as two units,
  its 543 `2.13` files and its 106 others (a unit mixing them is refused, #249); on a loaded
  machine they take 100.9 s and 7.3 s CPU and refuse 82 and 5 files, against 81.5 s CPU and 31
  refused file by file. scalac compiles its Scala and Java sources as one unit: its rows are
  not comparable.
- javalib's two refused files are left out after one typing of the unit (127.6 s CPU on a
  loaded machine), and scalac compiles it to class files without Scala Native's plugin: its
  rows are not comparable either.

## Ideas

1. **Deeper inlining** (ADR 0006 does small callees after tier-up): larger callees on hot
   paths.
2. **More superinstructions**, chosen with `SCIVOLO_PROFILE` instruction counts: a load or an
   operation fused with the branch or store after it. Removes one dispatch and one slot
   write each.
3. **Operand specialization.** `arg()` matches three operand kinds on every read. Emit
   variants per operand kind (slot/slot, slot/imm), or convert all constants into
   preloaded frame slots so every operand is a slot index.
4. **Closure compilation** (research report 03): each instruction becomes a boxed closure
   that captures resolved operands, and the dispatch `match` goes away. Reported gains are
   1.3–2× over a match-based loop. Larger rewrite; combine with 2 and 3.
5. **Unwind handling off the hot path** for calls and stores: they still go through
   `compute`, which records the pc and returns a `Flow` per instruction.
6. **Baseline JIT with Cranelift** (roadmap phase 2): translate hot lowered methods to machine
   code. Largest gain (10×+ on loops) and largest effort. Only worth it after 1–4 show their
   limits.
7. **Startup:** the library image and lazy decoding (ADR 0005), and the heap snapshot of the
   initialized runtime (ADR 0034). What remains for hello: ~2 ms process start, ~1.4 ms
   decoding the 1,093 definitions it uses, ~1.6 ms lowering 736 methods, ~1 ms laying out
   classes. Next: decoded definitions and lowered code in the snapshot too.
8. **Allocation:** boxing in generic collections allocates an `Integer` per element. Cache
   small boxed values the way `Integer.valueOf` does on the JVM (javalib may already do it;
   check).
9. **Exceptions:** every new `Throwable` snapshots up to 1,024 frames through the unwind
   externs (one native call per frame) and builds `StackTraceElement`s lazily. Code that throws
   in loops pays for it; a native `currentRawStackTrace` returning the snapshot as one array
   would cut the calls.
