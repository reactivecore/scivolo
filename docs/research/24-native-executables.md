# 24 — Native executables from NIR without Scala Native's toolchain (Cranelift)

Date: 2026-10-15. [V] verified by reading code or running it, [I] inferred. Paths: `SN/` =
`ref/scala-native/` (v0.5.12), `T/` = `SN/tools/src/main/scala/scala/scalanative/`, `R/` =
`SN/nativelib/src/main/resources/scala-native/`, `CL/` = `~/.cargo/registry/src/*/cranelift-*-0.135.6/src/`,
`V/` = `crates/vm/src/`. Machine: Apple M4 Pro, 12 cores, load average 4–10 during every
measurement (other sessions building), release builds, single-threaded codegen. Spike:
`spikes/aot-cranelift` on branch `spike-aot` (Draft MR !278), issue #113. Two mapping passes
(Scala Native's pipeline after NIR; the VM's runtime and the frontend's API) were done by
subagents and are condensed into §2 and §7 with their file:line evidence.

**Recommendation.** Cranelift 0.135 (`cranelift-codegen/frontend/module/object`) as the backend
in a new crate `crates/aot` with its own binary `scalai-aot`; flag-based exceptions; no stack
maps (Immix scans stacks conservatively); single-threaded; the library NIR compiled once into a
cached object with all dispatch data (class ids, vtables, itables, module slots) generated per
executable, so library code embeds no whole-program numbers. The spike turns the NIR of
`tests/programs/fib` into an arm64 Mach-O object in 2–3 ms, links it in 26 ms and runs
`fib(30)` correctly in 6 ms wall (VM 68 ms, Scala Native release binary 7 ms). The whole
library (9,677 files, 118k definitions, 975k NIR instructions) compiles in 6.5 s single-threaded
(6.7 µs per instruction, 74 µs per function, 98.8% of the functions) into a 34 MB object that
ld64 links in 80–110 ms. Expected end-to-end for a script executable: ~150–250 ms, 20× below
Scala Native's 3.4–5.6 s. Plan: 8 MRs, ~7–8k lines, §8.

## 1. Backend choice

| | Cranelift 0.135 | LLVM via inkwell | QBE / MIR / libgccjit |
|---|---|---|---|
| Rust integration | pure Rust crates, MSRV 1.95 [V `cargo info cranelift-codegen`] | links libLLVM (hundreds of MB, version-pinned) | C libraries over FFI; QBE is text-in/asm-out without a library API [I] |
| compile speed | 6.7 µs per NIR instruction, 74 µs per function, whole library 6.5 s on one core [V §5]; `opt_level=none` saves only 4% (6.3 s) [V] | Scala Native: 11 `.ll` files through clang + ld, 3.4–5.6 s end to end for hello (spike 00); literature: Cranelift ~10× faster than LLVM -O2 (research 03 §1.1) | fast; none has Cranelift's arm64 Mach-O object writer and ABI coverage [I] |
| code quality | fib(30) 6.0 ms at `speed`, 5.5 ms at `none` (noise level); SN release-fast 7 ms, SN debug 57 ms for cpu (spike 00); VM 68 ms [V] | best (≈15% over Cranelift, research 03) | QBE ≈ -O1 [I] |
| object output | `cranelift-object` writes ELF, Mach-O, COFF (`CL/object/backend.rs:59`) [V]; the writer applies the platform's C mangling, so `_S…` symbols are `__S…` on Mach-O [V spike] | yes | QBE emits assembly |
| exceptions | `try_call`/`try_call_indirect` exist (`CL/codegen-meta/shared/instructions.rs:315-362`) but only fill Cranelift-internal call-site tables ("Actual unwinding is outside the purview of the core Cranelift compiler", `:329-332`; `CL/codegen/machinst/buffer.rs:1757-1772`); cranelift-object emits no Mach-O `.eh_frame` (`CL/object/backend.rs:153-155`) [V] | full Itanium EH | partial |
| dynamic `alloca` | none: sized stack slots and `dynamic_stack_addr` for dynamic vector types only (`instructions.rs:1213-1215`) [V] | yes | yes |
| variadic C calls | not supported by Cranelift's call instructions [I: `Type::Vararg` bailed in the spike, 5 functions] | yes | yes |
| debug info | unwind tables on ELF/COFF via gimli; no DWARF line tables from cranelift-object [I] | full | partial |

Cranelift, with C shims for variadic externs and our own stack-trace walker. Not measured:
inkwell (libLLVM not installed; the spike's throughput is the comparison basis), lld and mold
(not installed; only Apple `ld` is on this machine).

## 2. Scala Native's pipeline after NIR, and what we have

Order (`T/build/Build.scala:127-161`, `T/build/ScalaNative.scala`): validate → `Reach` (link) →
re-link without multithreading if no `Thread.start` is reachable (`:204-242`) → Interflow
(`optimize`, then Reach again in memory, `ScalaNative.scala:53-74`) → codegen
(`GenerateReflectiveProxies` → `Metadata` → `Generate` → `ResourceEmbedder` → `Lower` → `.ll`
per source directory, `T/codegen/llvm/CodeGen.scala:26-62,87-94`) → clang per `.ll`
(`T/build/LLVM.scala:67-128`) → clang as the linker driver (`:219-336`, `-rdynamic`,
`-l pthread dl m` + every `@link`). Incremental mode re-emits only partitions whose defn hash
changed (`IncrementalCodeGenContext.scala:14-68`). Measured for `fib` (debug): Reach 2238 ms +
990 ms, Interflow 1160 ms, 701 classes / 3102 methods, 11 `.ll` files [V scala-cli `-v -v -v`
log]; totals 3.4–5.6 s (spike 00).

| step | Scala Native | VM today (`crates/vm`, 9,640 lines) | new in `crates/aot` | lines [I] |
|---|---|---|---|---|
| reachability | `T/linker/Reach.scala`: roots `:53-69, 257-311` (main, `<clinit>` of files with entry points), lazy virtual targets per allocated class `:428-489`, `method`/`dynmethod` `:883-920`, missing symbols `:933-946`, `Attr.Link/Define` `:681-687` | lazy loading on first use (`V/world/mod.rs:103`, `V/class/mod.rs:69`) | worklist over the script NIR and the image index (member tables, ADR 0005); lists reachable classes, modules, `<clinit>`s, externs, `@link`s; `Sig.Proxy` for `dynmethod` | 800 |
| class ids, ranges | `T/codegen/Metadata.scala:47-88` DFS preorder, arrays consecutive `:77-85`; traits coloured, `(index<<bits)|color|1<<31` (`TraitsUniverse.scala:8-91`) | link-order ids, `idRangeUntil = id` (`V/rtti.rs:83`), type tests by ancestor sets (`V/interp/dispatch.rs:136-161`) | per executable: preorder ids and ranges, RTTI structs (`CommonMemoryLayouts.scala:20-74`, C mirror `ObjectHeader.h:36-99`) in a tables object | 400 |
| vtables | `VirtualTable.scala:6-49`: parent's slots + one per sig with >1 target (whole-program) | hash map `(class, sig)` + 2-cell inline caches (`V/interp/invoke.rs:62-73`) | hierarchy-ordered slots (§6), `method` = load rtti, load slot (`Lower.scala:1116-1134`) | 300 |
| itables | `ITable.scala:12-77` power-of-two fast tables (mask) or sorted fallback; `R/itable_search.c:4-41` | none (`V/rtti.rs`: `itableCount`/`itable` stay 0) | tables per class; probe `itables[id & mask]` (`Lower.scala:906-955, 1270-1355`) with the trait id loaded from its rtti | 300 |
| dynmap | `DynamicHashMap.scala:14-25`, `PerfectHashMap.scala:169-222`, `R/dyndispatch.c:9-42`, proxies `GenerateReflectiveProxies.scala:11-180` | `V/interp/reflective.rs` | perfect-hash consts + proxies (`Lower.scala:1218-1268`) | 400 |
| Lower | `T/codegen/Lower.scala` (2,500 lines): slow paths `:89-100, 296-311, 371-498`, null checks `:215-219, 2008-2052`, `throw` `:517-533`, bounds `:642-662`, fields `:664-738`, calls `:1040-1097`, `method` `:1099-1216`, `is`/`as` `:1270-1394`, `classalloc` `:1424-1465` (`alloc_small` < 8192 B), `fptosi` saturation `:1467-1569`, div/rem/shift `:1577-1715`, box/unbox via `BoxesRunTime` `:1717-1765`, `module` `:1767-1790`, arrays `:1792-1915`, `stackalloc` + memset `:1917-1965`, strings `:1967-2006` | the same semantics in the interpreter (`V/interp/lower/*` 915 lines, `V/interp/memory.rs:98-119`, `V/interp/ops/arith.rs:72-106`, `V/interp/dispatch.rs:148-161`) | NIR → CLIF with these semantics; the spike's `translate.rs` (1,050 lines, stubs for the runtime ops) is the skeleton | 3,500–4,500 |
| Interflow | `T/interflow/*`; debug mode inlines `alwaysinline` and constructors only, release adds small (≤ 12 insts) and hinted callees, depth 32, callee ≤ 256 (`Inline.scala:14-91`, `OptimizerConfig.scala:39-44`); escape analysis (`Eval.scala:253-255`), poly-inline in release (`PolyInline.scala:10-53`) | tier-up devirtualization and inlining (ADR 0006, `V/interp/devirtualize.rs`, `inline.rs`) | none at first; later a NIR-to-NIR pass reusing the VM's `devirtualize`/`inline` logic in a whole-program `--release` mode | 0 (later ~1,500) |
| Generate | `T/codegen/Generate.scala`: `main` `:155-192` (GC init, every reachable `<clinit>` `:113-127`, `Runtime.init`, main, `onShutdown`, uncaught handler `:72-110`), module `load` accessors `:194-335` (slot store before the ctor), `__modules` `:337-353`, array/weak-ref ids `:364-464` | `V/session.rs:17-27`; `native/scalai/gc_glue.c` defines empty `__modules` and the ids | `main` in CLIF; module `load` functions; data symbols | 400 |
| metadata, stack traces | DWARF (`MetadataCodeGen.scala`); traces by libunwind (`R/platform/posix/unwind.c:21-80`, `runtime/StackTrace.scala:13-326`) | `V/natives/unwind.rs` emulates libunwind over frames | frame-pointer walk (`preserve_frame_pointers`) + a code-address → method table emitted with the object; no DWARF | 300 |
| exceptions | §3 | `Result` flag (`V/interp/unwind.rs:7-14`) | flags in CLIF; C `scalanative_throw` that sets the flag | 400 |
| externs, `@link`, `@define` | `Lower.scala:1040-1097` (blocking externs wrapped in GC state changes, multithreaded only); `LLVM.scala:225,250`, `NativeLib.scala:72-119` | dlsym + fixed-shape trampoline (`V/natives/ffi/mod.rs:112-154`), variadics `ffi/variadic.rs`, Rust overrides (`V/natives/runtime.rs:9-17,38-56`, `threads.rs`, `unwind.rs`, `resources.rs`, `llvm.rs`) | direct calls with real signatures; generated C shims for variadic externs and `llvm.*` intrinsics; `-l` flags; resources embedded like `ResourceEmbedder.scala:181-251` or read from the classpath as the VM does | 600 |
| stackalloc, pointers | `alloca` (`Lower.scala:1917-1965`) | frame stack (`V/stack/frames.rs`) | sized stack slots for constant counts (spike); a per-thread shadow stack for dynamic counts, reset on return and on handler entry | 200 |
| threads | yield points only with multithreading (`Lower.scala:865-904`), `R/MutatorThread.c`, `R/module_load.c:54-114` | green threads (ADR 0003) | none: single-threaded link, `isMultithreadingEnabled=false` (`V/linktime.rs:24-45,77-88`); threads later as OS threads with SN's own runtime | 0 |
| C runtime build | per-jar `scala-native/*.c` through clang (`LLVM.scala`) | `crates/snrt/build.rs`: one dylib from `native/`, `-DSCALANATIVE_GC_IMMIX`, no `eh.c`, `delimcc.c`, stack guards | a static archive target (same sources, `ar`), `eh.c` not needed | 100 |
| linker | clang driver | — | `cc` invocation from Rust (spike `main.rs:link`), `-dead_strip`/`--gc-sections`, optional `-fuse-ld=lld|mold` | 200 |

Total new: ~7,000–8,000 lines plus tests, below the issue's 12–20k estimate because the VM's
layout (`V/layout.rs`, `V/class/fields.rs`), `gcinfo.rs` reference offsets, `linktime.rs`
values, `crates/nir` and `crates/libimage` are reused as libraries [I].

## 3. Exceptions

Scala Native 0.5.12 [V]: default on Linux/macOS without LTO is `R/eh.c` (`T/build/Config.scala:206-213`):
`scalanative_throw(obj)` fills an `_Unwind_Exception` kept inside the Throwable's
`exceptionWrapper` blob (`eh.c:37-40, 278-324`; `nativelib/.../runtime/Throwable.scala:16-25`)
and calls `_Unwind_RaiseException`; the personality `scalanative_personality` (`eh.c:219-269`)
parses the LSDA itself and treats every call-site action as a catch-all; landing pads call
`scalanative_catch` (`T/codegen/llvm/compat/os/UnixCompat.scala:56-72`); every define carries
`personality` (`AbstractCodeGen.scala:317`). `eh.cpp` (C++ `throw`) only with LTO or on Windows.
A `throw` with a local handler is a plain jump, no unwinding (`Lower.scala:517-533`).

Options with Cranelift:
1. **Flag after calls** (the spike, recommended): a global (later thread-local) slot `rt_exc`
   loaded in the entry block; after each call `load; brif` to the handler (clears the slot,
   passes the object as the handler's parameter) or to one shared block returning zero values;
   `throw` without a local handler stores and returns. Cost [V §5]: whole-library object 33.8 MB
   with checks vs 28.8 MB without (+17%), compile 6.5 vs 5.5 s; fib(30) 6.0 ms with vs 7.0 ms
   without (noise). The VM runs 11,336 Scala Native tests with the same scheme (`V/interp/unwind.rs`).
   Interop: `native/` builds no `eh.c`; nothing in the C runtime throws into Scala today (the
   stack-overflow export `scalanative_throwStackOverflowError` and `delimcc.c` are not compiled);
   a C `scalanative_throw` that sets the flag and returns covers the generated code's own calls.
   C callbacks into Scala (`CFuncPtr`) return normally with the flag set; the C caller does not
   see it, the Scala caller of the C function checks after the call [I].
2. **`try_call` + Itanium tables**: zero cost on the happy path. Cranelift's payload registers are
   x0/x1 on arm64 and rax/rdx on x64 (`CL/codegen/isa/aarch64/abi.rs:1296-1302`, `x64/abi.rs:978-982`),
   the same as the Itanium landing-pad registers, and the Apple convention keeps callee-saves
   across `try_call` because "the compact unwind info properly describes how callee-save
   registers are restored" (`aarch64/abi.rs:1201-1203`) [V]; but `.eh_frame`/LSDA must be written
   by us with gimli (none on Mach-O, ELF without personality). ~800 lines later [I].
3. **setjmp/longjmp** per try region: regions must be reconstructed from the unwind edges, and
   longjmp over Rust frames is undefined; rejected.

## 4. GC

[V] `R/gc/immix/Marker.c:221-232` `Marker_MarkRoots`: per mutator thread the stack range
`stackTop..stackBottom` word by word plus the register buffer (`:156-200`), then `__modules`
(`:202-210`), then `GC_add_roots` ranges (`:212-219`, `GCRoots.c:10-45`), then precise heap
tracing through `refFieldOffsets` ending in -1 (`:87-128`, `MemoryLayout.scala:21-38`). No
stack maps and no safepoint metadata from the compiler. Stack bottom: `scalanative_GC_init`'s
local (`ImmixGC.c:26-41`) and `Runtime.init`'s stackalloc (`runtime/package.scala:61-65`),
the greater wins (`MutatorThread.c:128-135`). Yield points exist only with multithreading
(`Lower.scala:865-904`, `ImmixGC.c:181-186`). Allocation entry points: `scalanative_GC_alloc`,
`alloc_small`, `alloc_large`, `alloc_array(rtti, length, stride)` (`ImmixGC.c:43-81`).
Requirement on generated code: a live object must be reachable from a word holding its start
address (interior pointers are not recognized, research 03 §8.4); Cranelift keeps the base
value live while an `elem` address derived from it is live only if we keep it as a value or
block parameter — the lowering passes arrays, not element addresses, across calls [I, to verify
with `tests/programs/gcstress`]. Globals: `__modules` and the permanent-heap chunks as today
(ADR 0004); static fields are `Defn.Var` data symbols registered with `GC_add_roots` once at
startup (`V/gcinfo.rs` lists them for the VM).

## 5. Compile and link time (spike)

`spikes/aot-cranelift` (`cargo run --release -- [--opt none|speed] [--eh flag|none] [--verify]
[--link] [--run N] OUT.o DIR_OR_JAR…`): `nir` decode → CLIF (`translate.rs`): runtime ops are
calls to `rt_*` stubs (`c/rt.c`), field offsets are fake, module methods are devirtualized,
unwind edges become flag checks; `main.rs` links with `cc` from Rust and times it.

| | measured |
|---|---|
| fib program: 5 files, 14 functions, 109 NIR insts; read + decode | 0.3 + 0.2 ms |
| declare + translate + compile (speed, flag EH) | 0.3 + 2.0–2.7 ms (per-function fixed costs dominate) |
| emit object; ld64 (`cc`, rt.o + main.o + fib.o); binary | 0.1 ms; 26–32 ms (`-dead_strip` same); 50 KB |
| run `fib 30`, process wall (3 runs, first is cold) | 6.0 ms flag EH, 7.0 ms no EH, 5.5 ms `opt none`; output `fib(30) = 832040` |
| the same in the VM, SN | VM 68 ms (`just bench fib 10 30`), hello 12 ms, cpu 268 ms; SN release fib 7 ms (perf.md), build 3.4–5.6 s |
| whole library: 9,677 files, 27.9 MB, 118,019 definitions, 87,585 functions, 975,164 NIR insts; read / decode | 145–193 ms / 136–141 ms |
| translate + compile, `speed`, flag EH | 6,523 ms = 6.7 µs/inst, 74 µs/function; 1,035 bailed (1.2%: 966 duplicate erased names as spike 04 found, 28 use-before-def across blocks, 28 LinktimeIf edge cases, 13 varargs/struct values) |
| `opt none` / no EH checks | 6,281 ms, 34.2 MB / 5,517 ms, 28.8 MB |
| emit object | 65–68 ms, 33.8 MB |
| ld64 on the 33.8 MB object + stubs (nothing reachable from main) | 110 ms; `-dead_strip` 81 ms, binary 50 KB |

Codegen parallelizes per function (Cranelift contexts are independent): ~0.7 s on 10 cores for
the whole library [I]. Avoiding the external linker (writing Mach-O/ELF executables ourselves)
is not worth it at 30–110 ms; lld/mold would cut the big-object link further (not measurable
here: not installed).

## 6. Separate compilation of the library

What breaks when the library is compiled once, without the program: class ids and ranges
(preorder over the final hierarchy, `Metadata.scala:47-88`), vtable slots (sigs with >1 target,
`VirtualTable.scala:31-38`), trait colouring and itable masks, `__modules` indexes
(`ModuleArray.scala:8-33`), Interflow's single-target devirtualization and inlining, and the
linktime conditions (resolved per build, `LinktimeValueResolver.scala:60-142`).

Design [I]:
1. **Library code embeds no whole-program numbers.** `is[C]` loads `id`/`idRangeUntil` from
   `C`'s rtti symbol (2 loads + 2 compares instead of immediates); `is[T]`/trait dispatch load
   the trait id from `T`'s rtti; `module M` calls `M$load` (SN's own shape, `Generate.scala:265-288`)
   whose slot is a symbol; vtable slots are **declaration-ordered**: every virtual method of a
   class gets a slot, parents first, in a fixed order (declaration order in the NIR file), so a
   slot index depends only on the class's ancestry, which scripts can only extend by appending.
   Library code may then embed slot indexes as immediates. Cost: every virtual call in library
   code is a real vtable call (SN devirtualizes single targets); fib's 6 ms says it is affordable;
   a later whole-program `--release` mode (reachability + devirtualization + inlining over the
   NIR before codegen, 1–7 s) recovers it.
2. **Per-executable tables object**, written with the `object` crate directly (not Cranelift):
   rtti structs (`CommonMemoryLayouts.scala:20-74`) with ids, ranges, `refFieldOffsets`,
   vtables and itables for every *reachable* class; `__modules` as one array with a labelled
   symbol per slot (`_SM<mod>$slot`), so the GC scans it as today (`Marker.c:202-210`); array
   and weak-reference ids; string literals shared by name. Reachability over the image's member
   tables (ADR 0005) finds the classes and `<clinit>`s; hello touches ~1,100 definitions in the VM
   (perf.md), SN's fib 701 classes.
3. **Cached objects**: `libs-<key>.o` for the library image (all 118k definitions, 34 MB, one
   function per section so `-dead_strip`/`--gc-sections` keeps the binary at the reachable code;
   per-function sections are a `cranelift-object` option, `backend.rs:133`) plus one object per
   dependency jar, keyed like the image (canonical path, size, mtime, `build.rs:14`) plus the aot
   version and the linktime property set. Build on first use, ~7 s single-threaded or ~1 s
   parallel [I].
4. **Expected end-to-end for a script executable** [I]: frontend 20–45 ms (research 17) + script
   codegen 1–5 ms + reachability and tables ~10 ms + link 80–150 ms (the measured 81–110 ms grows
   with the reachable code) ≈ 150–250 ms, versus 3.4–5.6 s with Scala Native; the VM stays the
   edit-run loop at 20–50 ms.
5. **Risks**: ld time on Linux `ld.bfd` with a 34 MB object (mitigation: `mold`/`lld` when present,
   or splitting the library object per package so only referenced members are read); the
   declaration-ordered vtables must match between the cached object and the tables (hash the
   hierarchy into the key); duplicate erased names in the NIR (966) need the "last wins" rule of
   SN's reader (spike 04).

## 7. Executable layout: `scalai-aot`

CLI (scalac-like) [proposal]:
```
scalai-aot [-o BIN | -d OUT_DIR] [--main CLASS] [--dep org:name:ver]… [--cp DIR_OR_JAR]…
           [--release] [--link-flag FLAG]… SRC_DIR_OR_FILE…
```
Sources: every `.scala` file under a directory (sorted) and single files; NIR directories through
`--cp` (today's `scalai --cp`, `crates/scalai/src/main.rs:14-36`). Main class: the one
`static main(Array[String])` (`crates/scalai/src/mains.rs:6,23`, `D4mainLAL16java.lang.String_uEo`)
among the compiled units, `--main` when several (`script.rs:31-39` picks the same way).
Dependencies: Maven coordinates resolved with coursier to `_native0.5_3` jars as
`scripts/repl-deps.sh:18-23` does (`g::a::v` → `g:a_native0.5_3:v`, excluding scala-lang and
scala-native), then added to a per-project library image as the REPL does (`repl.rs:25-28`,
`libs::image(&jars)`): the image carries their `.nir` for codegen and `.tasty` with symbol tables
for the typer (`crates/tasty/src/library.rs:62`, `symbols/open.rs:19-20`). Today only the REPL
does this; `scalai FILE.scala` builds the image from the standard jars alone and puts `--cp`
jars into the VM classpath only (`main.rs:89-106`), so scripts cannot type against a dependency
jar: the aot path takes the REPL's route [V].

Frontend as a library [V]: `lower::compile(&mut Universe, &SourceFile, options) ->
Result<Vec<NirFile>, Refusal>` (`crates/lower/src/compile.rs:38-63`) parses, desugars, types with
`typer::type_unit` and lowers one file; `typer::Units::type_unit` (`crates/typer/src/lib.rs:58-114`)
types units one after another into one universe, each seeing the earlier units' symbols
(`syms.end_unit`, `:103`), which is how the REPL compiles inputs that use earlier classes
(`crates/repl/src/session/mod.rs:62`). Gaps for a multi-file unit: (1) `lower::compile` makes a
fresh `Units` per call; an `aot` driver must hold one `Units` and call `lower::lower` per file
(`crates/lower/src/lib.rs:117`); (2) forward references between files (A uses B, B uses A) fail
because symbols enter the universe unit by unit: the typer needs an "enter all units, then type
each" entry, dotty's namer order (~300 lines in `crates/typer`, its own MR); (3) cross-file
`inline` methods work only in that order [I]. Nothing in `crates/typer` or `crates/lower` has to
change for a single-file program.

Incremental rebuilds [proposal]: per source file a NIR cache keyed by the file's content hash,
the dependency set (image key) and the frontend version (`script.rs:43-69` does this for one
script); per dependency jar a cached object (§6.3); per build the tables object and the link.
A change in one file recompiles that file (and, until the typer has a cross-file dependency
graph, every file after it in the unit order — conservative, correct); nothing of the library
or the dependencies is recompiled.

Test strategy: (1) `tests/programs`: build every program as an executable and diff its output
with the `.check` file, like `scripts/program_runs.py` (`just aot-check`); (2) dotty `tests/run`:
the VM's `tmp/dotty/.../lower-results.tsv` is the oracle, a driver like `scripts/dotty-tests.py
native` builds and diffs; (3) unit tests of the tables against the VM's answers
(`V/interp/dispatch.rs` for method targets and type tests over the test programs' classes);
(4) `gcstress` and `threads` (expected refusal) as executables.

## 8. MR plan (≤ ~700 lines each, product code)

| # | MR | milestone | lines |
|---|---|---|---|
| 1 | `crates/aot` skeleton: `scalai-aot --cp DIR MAIN -o BIN`; NIR → CLIF for ints, control flow, direct and module-method calls (the spike's translator, cleaned); object + `cc` link; `crates/snrt` gains a static archive target | `fib`'s `fibo` runs from a C `main` | 650 |
| 2 | reachability over image + program NIR; tables object with the `object` crate: rtti, ids/ranges, declaration-ordered vtables, itables, module slots, array ids; `method`/`is`/`as`/`module` lowering | dispatch unit tests agree with the VM | 700 |
| 3 | heap: `classalloc`/arrays/boxes/strings/consts through `scalanative_GC_alloc*`, offsets from `V/layout.rs`; `main` with GC init, `<clinit>`s, `Runtime.init`, `onShutdown`; null/bounds/div/cast slow paths calling `runtime.package$.throwX` | **hello world executable** (`tests/programs/hello`) | 700 |
| 4 | exceptions as flags, `scalanative_throw` in C, stack traces by frame-pointer walk with a code-address table | `throws`, `tryfinally`, `traces` pass | 500 |
| 5 | externs with real signatures, variadic C shims, `llvm.*` intrinsic shims, `@link`/`@define`, `stackalloc` shadow stack, resources, linktime properties | all of `tests/programs` pass as executables (`just aot-check`) | 700 |
| 6 | library cache (`libs-<key>.o`, per-dependency objects), per-function sections and dead-strip link, `--dep` through coursier | cached hello links in < 250 ms | 500 |
| 7 | typer: enter all units before typing (`Units`), then `scalai-aot` over source directories with `-d`/`-o`/`--main`, per-file NIR cache, incremental rebuild | `tests/programs/*.scala` as directories; a two-file program with forward references | 400 + 400 |
| 8 | dotty `tests/run` driver and the fixes it finds | ≥ 95% of the VM's passing subset pass as executables | 500 |

Total ~5,500 product lines plus tests (~7–8k), 8–9 MRs, roughly 6–9 person-weeks [I]. Later,
outside the plan: zero-cost EH (§3.2), the whole-program `--release` mode (§6.1), OS threads
with SN's multithreaded runtime, Linux CI with `mold`.

Risks: variadic externs and struct-by-value C calls (Cranelift has neither: shims); stack
traces without DWARF (own table); interior pointers and conservative scanning (§4); ld time on
Linux; the typer's multi-unit entry (MR 7) touching `crates/typer`; the "last definition wins"
rule for duplicate erased names; `LinktimeIf` resolution must match the VM's property values
(`V/linktime.rs`) so programs behave the same in both.
