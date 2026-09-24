# 03 — Interpreter implementation architecture and runtime design (Rust focus)

Research memo for the Reactive Core "fast Scala 3 interpreter" project.
Target: Scala 3.9.0, Scala-Native-compatible subset, scripting use (startup latency first), extendable to precompiled code.

Date: 2026-09-24.

**Legend.** Every non-trivial claim is tagged:

- **[V]** verified against a cited primary source (paper, official docs, or source code fetched during this research).
- **[V-src]** verified by reading the actual source file (Scala Native / Scala 3 / MMTk repositories on `main`).
- **[I]** inference / engineering judgement; not directly measured or documented by a source.
- **[U]** could not be verified (source unreachable or ambiguous); treat as a lead, not a fact.

---

## 0. Executive summary

1. **Execution strategy.** Start with a *closure-compiled* ("closure-generating") interpreter over a lowered, resolved IR derived from TASTy, not a raw AST walker and not a bytecode VM. Measured data says closure compilation gets most of the win of a bytecode VM at a fraction of the engineering cost (Neil Mitchell: AST 2.1 s → closures 1.4 s → bytecode 1.5 s [V]); the OOPSLA'23 study found AST interpreters "on par with, or even slightly faster than" bytecode interpreters in interpreter-only mode [V]. The real wins are elsewhere: resolving names/overloads/dispatch *before* execution, lazy per-method compilation (Wasmi reports "several orders of magnitude" startup improvement from lazy translation [V]), and inline caches / quickening (CPython 3.11: +25 % average from PEP 659 & friends [V]). Keep a register-bytecode tier and a Cranelift baseline JIT as *later* options; design the IR so both remain possible.
2. **Object model.** Adopt Scala Native's object layout almost verbatim: header = `rtti*` (+ optional lock word), class RTTI with vtable, class-specific itables, `[idRangeFrom, idRangeUntil]` for `isInstanceOf`, arrays with `length` + `stride`, `java.lang.String` = `Array[Char]` + `offset` + `count` + `cachedHashCode` (UTF-16) [V-src]. This is what makes option 3(a) (calling Scala-Native-compiled code) possible later, and it is a perfectly good layout on its own.
3. **GC.** Do **not** use `Rc`/`Arc` (cycles are pervasive in Scala; RustPython's `Rc` approach has exactly this problem [V]). Do **not** use `gc-arena` for a general-purpose VM heap (its "mutation xor collection" model forces the interpreter to be stackless and returns to the arena boundary for every collection [V]). Recommended: a precise, non-moving mark-region collector of our own on raw memory, with **conservative stack scanning** for the interpreter's Rust stack (exactly what Scala Native's Immix does: it is "mostly-precise" — precise heap, conservative stack [V][V-src]). MMTk is the serious alternative if we want a production-grade collector family for free (Ruby 3.4's modular GC is MMTk-backed and does conservative stack scanning [V]); Boehm via `bdwgc-alloc` is the cheapest MVP.
4. **Interop with precompiled code.** Scala Native's `libraryDynamic` build target exports only `@exported` *static* functions with C-ABI types; managed objects do not cross the boundary, only reachable code is linked, and the library brings its own GC and `ScalaNativeInit` [V]. So option (a) "just call a Scala Native stdlib .so" does **not** work out of the box; it needs a modified Scala Native build (export all methods, share the GC). Option (b) — own ABI, own AOT path (TASTy/own-IR → Cranelift object files with the same layout) — is the coherent long-term design, and (c) heap snapshots is the cheapest big startup win and should be planned from day one (all of GraalVM native-image, Dart, Julia and Emacs do it [V]).
5. **Rust vs C++.** Rust is the right choice for the interpreter, IR, loader and AOT backend (Cranelift, MMTk, `libffi`, class-file/zip crates are all Rust-native [V]). GC/heap code is `unsafe` either way; the borrow checker does not help there but does not seriously hurt if the heap is a raw-pointer arena behind a small API. Reusing Scala Native's C/C++ runtime (Immix, `eh.cpp`) is possible via FFI but the value is low relative to the coupling.
6. **JVM bytecode.** Feasible as a *second* frontend but not as the primary one: Scala 3's JVM backend emits `invokedynamic`/`LambdaMetafactory` for lambdas [V], needs the real JDK class library (GPLv2+CPE; a `java.base`-only jlink image is ~30–50 MB [V]), and a credible Rust JVM (Ristretto) still reimplements every native method by hand [V]. Scala Native's javalib cannot simply be compiled to class files because it is written against Scala Native intrinsics [I]. Recommend: TASTy-first; keep class-file loading for *symbols/signatures* (a JAR on the classpath still has to be understood), not for execution, unless a later business case appears.
7. **Biggest scoping surprise.** TASTy is pickled *before* the `inlining` phase: only *transparent* inline calls are expanded in `Typer`; ordinary `inline def` calls and macro call sites are still unexpanded in `.tasty` [V-src]. An interpreter over TASTy therefore has to implement inline-method semantics itself (doable) and cannot run macros without embedding the compiler (not doable in Rust). This must be decided early — see §4.4.

---

## 1. Execution strategy

### 1.1 The options and what the measurements say

| Strategy | Startup cost | Steady-state speed (relative, from cited data) | Engineering cost | Evidence |
|---|---|---|---|---|
| AST tree-walking (naive, name lookup at runtime) | lowest | baseline (slowest) | lowest | Mitchell: 2.1 s [V] |
| AST interpreter with pre-resolved nodes / self-specialising nodes (Truffle style) | low | "on par with, or even slightly faster than" bytecode in interpreter-only mode [V] | medium | Larose/Kaleba/Burchell/Marr OOPSLA'23 [V] |
| Closure compilation (each node → `Box<dyn Fn>` / fn pointer + data) | low (one pass) | ≈ bytecode VM (1.4 s vs 1.5 s in Mitchell's Rust experiment) [V]; "~80 % of the win at a fraction of the risk" (lamedh issue, anecdotal) [V] | low–medium | Mitchell 2020 [V]; lamedh #200 [V] |
| Stack bytecode VM | one translation pass | Wasm: Wasmi 0.31 (stack) → 0.32 (register) roughly doubled Coremark (657→1457 AMD Epyc) [V] | medium–high | Wasmi blog [V] |
| Register bytecode VM | one translation pass, can be lazy | Wasmi 0.32 ≈ wasm3 (1457 vs 1309 Coremark, AMD Epyc); Stitch 1390; on Apple silicon Stitch (3056) ≫ Wasmi (1577) [V]. V8 Ignition: bytecode 25–50 % the size of baseline machine code [V] | high | Wasmi blog [V]; V8 Ignition blog [V] |
| In-place interpretation of the *existing* binary format + side table (Wizard) | very low (no rewrite; side table ≈ +30 % of bytecode size) | 1.7× slower than wasm3, others 2–3× slower [V] | medium | Titzer, OOPSLA'22 [V] |
| Specialising / quickening interpreter (inline caches) | none extra | CPython 3.11: +25 % avg, 10–60 % range on pyperformance; per-op 10–25 % [V]; Espresso also quickens bytecodes to specialised nodes [V] | incremental | Python 3.11 whatsnew [V]; Espresso docs [V] |
| Hand-tuned interpreter (LuaJIT asm; Deegen-generated) | none | LuaJIT interp ≈ 3× PUC-Lua; Deegen's generated interpreter beats LuaJIT's by 28 % [V] | very high | sillycross Deegen blog [V] |
| Baseline / template JIT (Winch, Sparkplug, BeamAsm, copy-and-patch) | compile ≈ 15–20× faster than optimising compiler [V] | 1.1–1.5× slower than optimising compile [V]; Sparkplug +45 % JetStream over Ignition [V]; BeamAsm +30–130 % over BEAM interpreter [V]; copy-and-patch 39 % faster than Liftoff on Coremark [V] | high (per-arch) | Wasmtime baseline RFC [V]; V8 Sparkplug [V]; Erlang JIT docs [V]; Xu & Kjolstad 2021 [V] |
| Optimising JIT via Cranelift | compile ≈ 10× faster than LLVM in Wasm engines (context dependent; 20–35 % in other measurements) | ~14 % slower code than LLVM (WAVM), ~2 % slower than TurboFan [V] | high | Wasmtime 1.0 perf article / survey [V] |

Additional data points relevant to *our* language shape:

- **AST interpreters are not the losers folklore says they are.** The OOPSLA'23 study built matched AST and bytecode interpreters for the same language on both Truffle and RPython. Interpreter-only: AST on par or slightly faster; bytecode is more compact for large programs but "allocate[s] more memory because boxing avoidance is not as applicable" and needs a reified stack [V]. Their conclusion: bytecode interpreters "do not have their widely assumed performance advantage" [V]. For a statically typed language like Scala, boxing avoidance is even more attractive (we know the static types).
- **Dispatch overhead is the thing to minimise, not the tree-vs-bytecode question.** Wasmi's rewrite explicitly attributed the 2× to fewer, fatter instructions (4 stack ops → 1 register op) [V]; supernodes in Truffle AST interpreters cut runtime up to 42 % [V]; Ignition's register design exists to cut "trips through the interpreter's dispatch loop" [V]. Closure compilation achieves the same "one dispatch per semantic operation" effect naturally, and Mitchell notes Rust "is able to turn tail calls on dynamic functions into jumps" so predicted indirect calls are cheap [V].
- **Truffle without a JIT is slow** in absolute terms; the whole Truffle premise is that partial evaluation removes the interpreter overhead. Espresso on a plain OpenJDK host "will only execute in the interpreter and is thus slow" [V]. We have no Graal, so we should not copy Truffle's node-rewriting machinery wholesale; we should copy its *ideas* (specialised nodes, inline caches).

### 1.2 Startup latency is a different axis

For scripts the wall-clock budget is dominated by *before-first-instruction* work: reading and resolving the program and its library closure, initialising module objects, and (if any) code generation. The data:

- Wasmi: lazy function translation gave "several orders of magnitude" startup improvement; linker caching 120× [V]. → Compile methods lazily on first call; never eagerly lower the whole stdlib.
- Wizard: interpreting the shipped format in place avoids a rewrite pass entirely, with a side table of ~30 % extra space [V]. → TASTy is *not* a good in-place format (it is a typed tree with symbol tables; name/overload resolution is needed before execution), so we cannot go fully Wizard-style. But we can keep the *lowered* form compact and lazily built.
- CPython 3.11 got 10–15 % faster startup just from freezing core modules into static code objects instead of unmarshalling `.pyc` [V]. → The stdlib's *lowered IR* should be cacheable on disk (mmap-able), and ideally its *initialised heap* too (see §3, snapshots).
- V8's rationale for Ignition was memory and eager-compile cost: bytecode is 25–50 % the size of baseline machine code, so "run-once or non-hot code" can be compiled eagerly and cheaply [V]. → Machine-code JIT is never the startup answer; a JIT only pays off for hot loops.
- JVM baseline for reference: Scala "hello world" ≈ 300 ms on JVM vs ≈ 100 ms Java vs 1 ms native C (old but representative data) [V]; Scala Native binaries start in single-digit ms [I]. The target for the interpreter should be "tens of ms" for a small script including stdlib load, which is only achievable with lazy loading + cached/snapshotted stdlib.

### 1.3 Recommendation

**Phase 1 — closure-compiled interpreter over a resolved IR ("LIR").**

- Frontend: TASTy → a fully *resolved* per-method IR: symbols resolved to slots/indices, overloads picked (TASTy already carries the selected symbol), local variables numbered, pattern matches lowered to tests+extractors, `while`/`if`/`try` explicit, tail-recursive self-calls turned into jumps (Scala's `@tailrec` guarantee holds only for self-recursion; the compiler has already checked it, we implement the loop) [I].
- Execution: each LIR node compiles lazily (on first call of the enclosing method) to a closure tree (`Box<dyn Fn(&mut Frame) -> Value>` or a small enum of pre-bound node structs with `#[inline]` `eval`). Locals live in a per-frame `Vec<Value>`/slice; `Value` is a tagged 16-byte enum or (better, later) NaN-boxed/pointer-tagged 8-byte word [I].
- Inline caches: monomorphic `(rtti, method_ptr)` caches at virtual/interface call sites and `isInstanceOf` sites, using `Cell` inside the closure's captured state — the CPython 3.11 / Espresso quickening idea [V] applied to closures.
- Boxing avoidance: because Scala is statically typed and TASTy is pre-erasure, primitive-typed expressions can evaluate through unboxed fast paths (`eval_int`, `eval_long`, `eval_double` alongside `eval`) — the "boxing avoidance is more applicable to AST interpreters" result [V].

Why not straight to a register VM: engineering cost is 2–3× and, per OOPSLA'23 and Mitchell, the runtime win over a well-built closure interpreter is small [V]. Why not a raw AST walker: name lookup and dispatch resolution at runtime are what make naive walkers slow (Mitchell's biggest step was removing runtime variable lookup [V]).

**Phase 2 — profile-guided quickening and a compact serialisable IR.** Persist the LIR for the stdlib to disk; add per-site specialisation (int arithmetic, `Array[Int]` access, string concat).

**Phase 3 (optional) — baseline JIT with Cranelift** for hot methods, from the *same* LIR. Cranelift is the natural Rust choice (`cranelift-jit` exists but is labelled "extremely experimental" [V]; Wasmtime uses `cranelift-codegen` in production). The Winch RFC numbers (baseline 15–20× faster compile, 1.1–1.5× slower code than optimising) [V] bound what a template tier buys. Since the primary goal is startup, this tier is only for long-running scripts and should share the object layout with the AOT path (§3).

---

## 2. Object model and memory management (Scala in Rust)

### 2.1 What Scala Native actually does (verified from source)

Read from `tools/src/main/scala/scala/scalanative/codegen/CommonMemoryLayouts.scala` on `main` [V-src]:

```scala
object ObjectHeader extends Layout(
  nir.Type.Ptr ::               // RTTI
  meta.lockWordType.toList)     // optional, multithreading only

object ArrayHeader extends Layout(
  nir.Type.Ptr ::               // RTTI
  meta.lockWordType.toList :::  // optional, multithreading only
  nir.Type.Int ::               // length
  nir.Type.Int :: Nil)          // stride (used only by GC)

object Rtti extends Layout(
  nir.Type.Ptr ::               // ClassRtti (the RTTI's own RTTI — RTTI objects are java.lang.Class instances)
  meta.lockWordType.toList :::  // optional
  nir.Type.Int ::               // ClassId
  nir.Type.Int ::               // InterfacesCount
  nir.Type.Ptr ::               // Interfaces
  nir.Type.Ptr :: Nil)          // ClassName

object ClassRtti /* = Rtti ++ */ (
  Int   class size,
  Int   id range (idRangeUntil),
  Ptr   reference offsets (GC map),
  Int   itableSize,   // negative => slow-path (binary search in itable_search.c)
  Ptr   itables,
  Ptr   superClass,
  [DynamicHashMap]    // only if structural/reflective calls exist
  Ptr[] vtable)       // variable length

object ITable = (Int id, Ptr[itableSize])
```

Key properties:

- **Header is one word** (`rtti*`) in single-threaded builds, two words with multithreading (lock word for monitors) [V-src]. `__getClass()` is literally "load the first word" [V-src, `runtime/Object.scala`]. Identity `hashCode` is address-derived (`addr.toInt ^ (addr >> 32)`) — which implies a **non-moving** heap [V-src][I].
- **RTTI objects are the `java.lang.Class` objects** (the first field of an RTTI is a pointer to the RTTI of `Class`) [V-src].
- **`isInstanceOf` for classes** is a range check on class ids (`ClassId`, `idRangeUntil`) — the classic pre-order numbering trick; O(1) [V-src]. **For traits**, 0.5.7 replaced a single global trait-dispatch table with per-class itables ("class-specific itables to reduce the size of binaries") [V]; a class's itable size is a power of two used as a mask for fast lookup, or negative meaning "binary search" (`itable_search.c`) [V-src].
- **Virtual dispatch** is a plain vtable at the end of the class RTTI [V-src]; **structural (reflective) calls** use a per-class dynamic hash map (`dyndispatch.c`, `DynamicHashMap.scala`) [V-src].
- **Arrays** carry `length` and `stride` (element size, used by the GC to scan reference arrays) [V-src].
- **GC map**: `reference offsets` is a list of field offsets holding references, used by the precise heap scan [V-src, `MemoryLayout.scala` "A list of offsets pointing to inner fields of reference types"].
- **Strings**: `javalib/src/main/scala/java/lang/String.scala` declares `final class _String` with `var value: Array[Char]`, `var offset: Int`, `var count: Int`, `var cachedHashCode: Int` [V-src]. So Scala Native strings are UTF-16 with the old (pre-JDK 7u6) offset/count sharing design — substrings can share the backing array. There is no compact-strings (Latin-1 `byte[]`) optimisation.
- **Exceptions**: two runtime variants. `eh.cpp` (when `SCALANATIVE_USING_CPP_EXCEPTIONS`): `void scalanative_throw(void *obj) { throw scalanative::ExceptionWrapper(obj); }` — Scala exceptions are C++ exceptions wrapping the object; only `ExceptionWrapper` can be caught by Scala code [V-src]. `eh.c` (default, no C++ EH): a hand-rolled Itanium-ABI `_Unwind_Exception` embedded in an `ExceptionWrapper` struct, raised with `_Unwind_RaiseException` (unwind.h) [V-src]. Either way it is zero-cost (table-driven) unwinding through LLVM `invoke`/`landingpad`; NIR has `invoke`/`throw`/`unwind` instructions [V].
- **Delimited continuations**: `delimcc.c` implements delimited continuations (used for virtual threads / `VirtualThreadScheduler.scala`) with stack copying and `setjmp/longjmp` escape for exceptions crossing a continuation [V-src]. This is a warning: the Scala Native ABI is *not* just "C with a GC".
- **GC**: Immix is the default; the docs describe it as "a mostly-precise mark-region tracing garbage collector" [V]. `Marker.c` has `Marker_markConservative(...)` called from `Marker_markRange(...)` (stack/registers/extra roots) and precise `Marker_markField` for heap objects via the RTTI reference-offset map [V-src]. Commix is the parallel/concurrent variant; Boehm and "none" are the other options; Immix/Commix support multithreading with safepoint polling (`SafepointPollTrampoline-*.S`, `YieldPointTrap.c`) [V-src]. The Immix core is C (`gc/immix/*.c`), the EH shim is C/C++.

### 2.2 Proposed object model for the interpreter

Adopt the Scala Native layout as the *ABI of the project* (whether or not we ever link Scala Native binaries), with these decisions:

1. **Header**: `rtti: *const Rtti` + `lock/hash word: u64` (always present; a stable 2-word header is worth it for identity hash, monitors, GC mark/forward bits and future compatibility with Scala Native's multithreaded layout) [I].
2. **RTTI**: same field order as Scala Native's `ClassRtti` (id, id range, size, ref-offset map, itables, superclass, vtable) so that a future AOT backend or a Scala Native `.so` can share it [I]. Add interpreter-only side data (method LIR, constant pool, source positions) in a *separate* Rust struct pointed to from the RTTI — never inside the ABI part.
3. **Dispatch**: vtable index resolved at load time from the TASTy symbol; interface calls through per-class itables with a monomorphic inline cache in front. Structural calls (`Selectable`/`reflectiveSelectable`) via the dynamic hash map.
4. **Boxing**: Scala's boxes are `java.lang.Integer` etc. (`runtime/Boxes.scala` in Scala Native) [V-src]. Keep the interpreter's `Value` unboxed for primitives; materialise a real box object only when a primitive flows into a reference-typed slot (`Any`, generic `T`, collections). Cache small `Integer`/`Boolean`/`Char` boxes like the JVM does [I].
5. **Arrays**: exactly Scala Native's header (`rtti, lock, length, stride`) followed by inline elements, one RTTI per primitive array type plus `ObjectArray` [V-src][I].
6. **Strings**: UTF-16 `Array[Char]` + offset/count/hash, matching Scala Native [V-src]. Do **not** try to use Rust `String` (UTF-8) as the representation: `charAt`, `length`, `substring`, `indexOf`, surrogate handling, `StringBuilder` and `hashCode` (which is specified over UTF-16 code units) all assume UTF-16. Convert at the OS boundary only. Rust's `Vec<u16>` + the `widestring`/`utf16` helpers are enough [I].
7. **Closures**: Scala 3 lambdas are anonymous classes implementing `FunctionN`/SAM types after the compiler's `LambdaLift`/`ExpandSAMs` phases — but TASTy is *before* those phases, so the interpreter sees `Lambda`/`Block(DefDef, Closure)` trees [V-src, `LAMBDA`/`INLINED` tags in `TastyFormat.scala`]. Represent a closure as an ordinary object whose RTTI is a synthetic class with one vtable entry per SAM method and captured variables as fields; captured `var`s need boxing into a `Ref` cell exactly as scalac does [I].
8. **Exceptions**: interpreter-level unwinding via `Result<Value, Throw>` (or a thread-local pending-exception flag checked after calls) — cheap on the non-throwing path, no dependence on native unwinding. When native precompiled code is called (§3), catch at the FFI boundary and convert [I]. Zero-cost native EH (à la `eh.c`) only matters for the AOT tier.
9. **Tail calls**: `@tailrec` self-calls become loops in LIR. General TCO is not needed (Scala does not promise it) [I].
10. **Monitors / threads**: single-threaded first (`lockWordType = None` mode), but keep the header slot. Scala Native's virtual threads via delimited continuations are out of scope.

### 2.3 GC options in Rust

| Option | Precision | Moving | Rust ergonomics | Notes |
|---|---|---|---|---|
| `Rc`/`Arc` (RustPython style) | n/a | no | best | Leaks cycles; RustPython's `PyObjectRef` is `Rc<...>` and "like CPython... can't handle cycles" [V]. Scala code is full of cycles (parents/children, closures capturing `this`, mutable collections). Reject. |
| `gc-arena` (Ruffle, piccolo) | exact | no (incremental mark-sweep modelled on Lua 5.4) | safe API, branded lifetimes | "Mutation xor collection": `Arena::mutate` must *return* for a collection to run; interpreter must be stackless/trampolined (piccolo does exactly this and admits its VM "sorely needs optimization") [V]. Great for embedding a scripting VM in a game loop, wrong shape for a fast recursive closure interpreter with deep Scala call stacks. Reject for the main heap. |
| `rust-gc` (`gc` crate), `boa_gc`, `dumpster`, `shredder`, `safe-gc` | exact / cycle-collecting RC | no | safe | Thread-confined, `Trace`-derive based; `safe-gc` states it "is not a particularly high-performance garbage collector" [V]. Fine for tools, not for a VM heap that must hold millions of small objects with a fixed ABI layout. Reject. |
| Boehm (`bdwgc-alloc`, `boehm_gc` crates) | conservative | no | trivial (`GlobalAlloc` impl) [V] | Scala Native still ships Boehm as an option [V]. Excellent MVP: zero GC engineering, correct with any Rust code holding raw pointers on the stack. Costs: conservative heap scanning unless we use typed allocation (`GC_malloc_explicitly_typed`), no generational/compaction, slower allocation. Keep as a build-time selectable backend. |
| Own precise non-moving mark-region (Immix-like) with conservative stack roots | heap precise (RTTI ref-offset map), stack conservative | no (or opportunistic evacuation later) | all `unsafe`, but contained in one module | This is what Scala Native does [V][V-src]. Conservative stack scanning removes the need for stack maps or `Rooted<T>` handles in every Rust function — the single biggest source of GC pain in Rust VMs (Servo/SpiderMonkey-style rooting). MMTk's Ruby binding shows it can be done efficiently ("MMTk efficiently implemented conservative garbage collection following ... Shahriyar et al.") [V]. |
| MMTk (`mmtk` crate) | precise heap; conservative roots supported via pinning | plans: NoGC, MarkSweep (non-moving), Immix (opportunistically moving, supports pinning), SemiSpace, generational | Rust-native; binding must implement `ObjectModel`, `Scanning`, `ActivePlan`, `Collection`, `ReferenceGlue`, `Slot`, `MemorySlice` [V] | Used by OpenJDK, JikesRVM, Julia (Rust binding), Ruby 3.4 (Rust binding) [V]. Object metadata can be in-header or in side metadata [V]. Gives us a whole family of collectors; the cost is the binding surface and MMTk's own conventions (heap layout, `ObjectReference` alignment, `bind_mutator`, safepoint protocol) [V]. |

**Recommendation.**

- **MVP:** Boehm through `bdwgc-alloc` (or direct `GC_malloc` FFI) so that the interpreter can be built and benchmarked without writing a collector. All heap objects already carry an RTTI with a reference map, so switching later is mechanical.
- **Target:** own Immix-style mark-region, non-moving, precise-heap / conservative-stack collector in Rust, sized for scripts (small nursery-less heap, fast bump allocation in lines/blocks, lazy sweeping). Non-moving keeps address-based identity hash trivial (as Scala Native does) and keeps raw pointers in Rust code valid across collections — which is what makes Rust *comfortable*: the interpreter can hold `*mut Object` in locals, in `Value`s, and in inline caches without rooting discipline. Conservative stack scanning of the Rust stack requires: known stack bounds per thread, a register spill (`setjmp` or an asm stub, cf. Scala Native's `RegistersCapture.h`) [V-src], pointer-aligned scanning, and an "is this a valid object start?" test (Scala Native uses a byte map per line; MMTk has `is_mmtk_object`) [V-src][V].
- **Alternative if we want generational/parallel GC without writing it:** MMTk. The binding effort is real (seven traits, side-metadata conventions) but well-trodden; Ruby's practitioner report is the best guide [V]. Decide after the MVP, based on measured allocation profiles of real scripts. Because scripts are short-lived, a nursery-only ("sticky") scheme might beat a full generational collector for our workload [I].
- **Never** expose `Value`/object pointers as `Rc`, and never let GC objects own Rust heap allocations that need `Drop` — everything reachable from the GC heap must be GC-allocated or explicitly finalised (weak-ref/finaliser lists as in Immix's `WeakReferences.c`) [V-src][I].

---

## 3. Interop with precompiled code

### 3.1 What Scala Native provides today (verified)

- Build targets `application`, `libraryDynamic`, `libraryStatic` [V]. A dynamic library is "built based on entry point methods annotated with `@exported`"; "Only statically reachable functions can be exported in your library"; "Currently, only static object methods can be exported"; module fields cannot be exported, only accessors via `@exportAccessors` [V].
- Exported signatures are C-ABI: primitives, `CString`, `Ptr[...]`, `CStruct`, `CFuncPtr`; "no managed Scala objects cross the boundary" (docs examples show only primitive/`CString` returns) [V].
- `ScalaNativeInit()` must run before any Scala code; for a dylib a constructor runs it automatically unless `SCALANATIVE_NO_DYLIB_CTOR` is set (`dylib_init.c`: `__attribute__((constructor))` / `DllMain`) [V][V-src].
- The GC (Immix by default) is linked *into* the library and manages the library's heap; it scans the whole thread stack conservatively and registers extra roots via `GC_Roots_Add(range)` [V-src].
- Whole-program linking: the NIR linker only includes definitions reachable from entry points; interflow optimises across the program [V]. There is no "compile the whole stdlib as a library with every method exported" mode.

### 3.2 The three options

**(a) Adopt Scala Native's layout + GC and call into a Scala Native `.so`.**

- *Feasibility today:* not with stock Scala Native. `@exported` is limited to static methods with C types, and reachability-based linking would strip everything not reachable from those exports [V]. Passing a managed object requires both sides to agree on the header *and* the RTTI identity (`isInstanceOf` compares class ids and RTTI pointers; class ids are assigned per link [V-src]) — so the interpreter would have to *load its class metadata from the Scala Native binary* rather than from TASTy, or at least map its own class table onto the binary's.
- *What it would take:* a modified Scala Native build (or a NIR-level tool) that (1) treats every public method of the stdlib as an entry point, (2) emits a symbol table (name → address, class → RTTI address, vtable/itable layouts, field offsets), (3) shares one GC: either the interpreter allocates through the library's `scalanative_GC_alloc` and registers its own roots via `GC_Roots_Add`, or the library is built with `GC=none` and the interpreter's GC owns everything (then the library's allocation calls must be redirected). Exceptions thrown inside the library use `_Unwind_RaiseException`/C++ `throw` [V-src]; a Rust frame in between must be unwind-safe (Rust frames are not by default — `extern "C-unwind"` and catching at the boundary needed) [I].
- *Verdict:* high coupling to Scala Native internals (layout, class-id assignment, GC, EH, delimited continuations), and any Scala Native version bump can break it. Not a foundation; possibly a research spike later.

**(b) Own ABI + own AOT path (same layout for interpreted and compiled code).**

- The interpreter defines the layout (§2.2, deliberately Scala-Native-shaped). A second backend lowers the *same LIR* to machine code via Cranelift (object files or in-process JIT). Interpreted and compiled methods share RTTI, vtables, heap, GC and calling convention (one Rust `extern "C"` convention: `fn(frame_or_args...) -> Value`, or a compiled-code-specific convention with a thunk for interpreter↔native transitions). This is how every serious mixed-mode VM works (HotSpot, V8, Dart, BEAM: the JIT/AOT output is just another implementation of the same method slot) [V][I].
- Cost: a code generator (Cranelift makes this tractable in Rust; Winch-style template codegen from LIR is a few thousand lines [I]) and a binary format for "precompiled module + serialised metadata" (Dart's clustered snapshot with an mmap-able code section is the model [V]).
- Benefit: no dependency on Scala Native's runtime; the stdlib can be shipped as (LIR + optional machine code) produced by *our* toolchain from the published TASTy.

**(c) Heap snapshot of a pre-initialised image.**

- Precedents: GraalVM native-image writes all objects reachable from build-time static initialisers into an *image heap*, iterating with points-to analysis to a fixed point, so "your application starts much faster with a pre-populated heap" [V]. Dart AppJIT snapshots capture "all the parsed classes and compiled code generated during a training run", so the VM "starts running user code sooner"; AOT snapshots mmap a code section that "directly become[s] part of the heap" [V]. Julia sysimages are "a shared library where the state of a running Julia session has been stored" and are "not relocatable to other machines" [V]. Emacs pdumper "walks the Emacs heap and writes the data, along with necessary relocation, to a file", requiring no `unexec` and working with PIE/ASLR, but dumps "can be loaded only by the Emacs executable that dumped them" [V].
- For us: after loading the stdlib's TASTy, resolving symbols, building RTTIs/vtables and running module initialisers that are pure (`scala.Predef`, `scala.collection` companions, `scala.math`, ...), serialise the heap + metadata into a relocatable image (pointer table + mmap; or position-independent by storing heap offsets and relocating on load like pdumper). Loading a 10–30 MB image by `mmap` is milliseconds; rebuilding it from TASTy every run is what the JVM effectively does (and why it takes 300 ms) [V][I].
- Risks: module initialisers with side effects (System properties, environment, time) must be excluded or re-run (GraalVM defaults to runtime initialisation and only build-time-initialises provably safe classes [V]); image is tied to the exact interpreter build (fine: we control both).

### 3.3 Mixed-mode designs elsewhere (what to copy)

| System | Mixed-mode design | Lesson |
|---|---|---|
| GraalVM Espresso | JVM bytecode interpreter written in Java on Truffle; guest objects are host `StaticObject`s; host GC; reuses OpenJDK's class library with native methods either *substituted* in host code or routed via Truffle NFI/Sulong; fastest startup when the interpreter itself is native-image-compiled [V] | Reusing a foreign stdlib forces you to own its native surface. Startup comes from AOT-compiling the *interpreter* and snapshotting, not from JITting user code. |
| PyPy + cpyext | emulates CPython's C API for extensions; "cpyext is slow and always will be slow because it has to emulate far too many details of the CPython object model" [V]; crossing the boundary is invisible to the JIT [V] | An ABI mismatch between interpreter and precompiled code is a permanent tax. Choose one object layout for both sides (supports (b) over (a)-with-adapters). |
| Dart VM | kernel (AST) binaries with lazy deserialisation; AppJIT snapshot after a training run; AOT snapshot with TFA-pruned code; clustered serialisation [V] | Lazy deserialisation + training-run snapshots are the practical startup path for an interpreter. |
| OCaml | bytecode (`ocamlrun`) and native (`ocamlopt`) are separate worlds: "It is not possible to mix native-code object files produced by ocamlopt with bytecode object files produced by ocamlc" [V] | Do not build two disjoint runtimes; one layout, two code shapes. |
| Julia | sysimage = serialised session as shared library; frozen packages; not relocatable across machines [V] | Snapshots are worth version/machine pinning for latency. |
| Erlang/OTP | BEAM interpreter; HiPE (2–3× on sequential code) removed in OTP 24 in favour of BeamAsm, a load-time template JIT with "hardly any cross instruction optimizations", +30–130 % [V] | A simple load-time template JIT sharing the interpreter's register model is a sustainable design; an ambitious separate native compiler (HiPE) was not. |
| V8 | Ignition register bytecode → Sparkplug single-pass baseline (+45 % JetStream vs Ignition) → Maglev/TurboFan; snapshots for the initial heap [V] | Tiering only pays for long-running code; the interpreter+snapshot is the startup story. |

### 3.4 Recommendation for interop

1. Design the object layout now to be Scala-Native-compatible in the parts that matter (header, RTTI, arrays, strings) — it costs nothing and keeps (a) open for research.
2. Implement (c) heap snapshots early (Phase 2): it is the largest startup win, independent of any compiler work.
3. Implement (b) as the AOT/JIT path when hot-loop performance matters, from the same LIR, with Cranelift. Ship the stdlib as LIR (+ snapshot), optionally + machine code.
4. Treat (a) as an experiment: "can the interpreter load the RTTI/symbol tables of a Scala Native `libraryStatic` build and call non-exported functions by address, sharing Immix?" — only if a concrete need (e.g. reusing Scala Native's `java.util.regex`/RE2 or `java.nio` implementations) appears.

---

## 4. Rust building blocks

| Need | Crate / project | Status (verified) | Notes |
|---|---|---|---|
| TASTy reader | **none in Rust** — searches for "TASTy" on crates.io/lib.rs only surface an unrelated `tasty` (TOML API test runner) [V] | must write | Format spec is `TastyFormat.scala` (MajorVersion 28, MinorVersion 10, ExperimentalVersion 1 on `main` at time of writing) [V-src]; JVM reference implementations: dotty's `TastyUnpickler`/`TreeUnpickler`, Scala Center's `tasty-query` (compiler-independent, resolves types; also reads Java class files and Scala 2 pickles for symbols) [V]; shardulc's `tasty-interpreter` is a Scala reference interpreter for TASTy (README unreachable during research) [U]. |
| Scala source parser | `tree-sitter-scala` (Scala 2 and 3; CI smoke-parses `scala/scala` and `dotty` sources) [V] | usable for syntax only | No typing; a TASTy-based interpreter does not need it except for tooling/REPL sugar. Full Scala 3 typing in Rust is out of scope (implicits, givens, inference) [I]. |
| Java class-file parsing (for JAR symbol info, Java interop signatures) | `cafebabe` (Java 21 format, zero-copy `Cow<str>`), `noak` (read+write), `classfile-parser` (nom), `ristretto_classfile`, `mokapot` [V] | mature | Needed even in a TASTy-first design to read *Java* dependencies' signatures (tasty-query does the same) [V][I]. |
| JAR/zip reading | `zip`, `rc-zip` (sans-io), `rawzip` (lazy central directory, zero deps) [V] | mature | Coursier cache layout is on disk in `~/.cache/coursier`; scripts will pull the Scala 3 library JAR (which contains both `.class` and `.tasty`). |
| Code generation | `cranelift-codegen`/`cranelift-frontend` (Wasmtime production), `cranelift-jit` ("extremely experimental" per its own README) [V]; `cranelift-object` for `.o` output | active (0.127 in Dec 2025) [V] | ~10× faster than LLVM to compile in Wasm engines; ~14 % slower code than LLVM [V]. |
| LLVM (if ever) | `inkwell` (LLVM 11–22), `llvm-sys` [V] | active | Only for an "AOT via LLVM to match Scala Native" experiment; heavy dependency. |
| GC | `mmtk` (bindings: OpenJDK C++, Julia Rust, Ruby Rust) [V]; `bdwgc-alloc` (Boehm `GlobalAlloc`) [V]; `gc-arena` [V] | see §2.3 | |
| FFI to C (JNI-like natives, `@extern`) | `libffi` crate (bindings; active Dec 2025) [V] | mature | Needed for calling arbitrary C signatures from interpreted code (Scala Native's `@extern` subset). |
| JVM in Rust (reference / fallback) | `ristretto` (`ristretto_vm`, `_classfile`, `_jit` for x86-64/aarch64/s390x/riscv64; runtime classes "backed by LTS versions of AWS Corretto"; native methods reimplemented in Rust, unimplemented ones throw `UnsatisfiedLinkError`; Apache-2.0/MIT) [V] | active | Evidence that a Rust JVM is possible and that the long tail is native methods. |
| Lua-VM-in-Rust precedent | `piccolo` (stackless, gc-arena) [V] | experimental | Demonstrates the cost of the arena model. |

### 4.4 TASTy-specific scoping facts (important)

- TASTy is **pre-erasure** and **pre-transform**: it is what `Typer`+`PostTyper` produce, pickled by the `pickler` phase before `inlining`, `erasure` and all other transforms [V]. Consequences: the interpreter must itself do pattern-match lowering, lambda lifting/closure conversion, `Unit`/value-class handling, default arguments (already desugared into `$default$n` methods by typer — fine), by-name parameters, `lazy val`, and erasure decisions (which vtable slot does a generic method occupy?). Scala Native's NIR is *post*-erasure, so the Scala Native ABI and TASTy are separated by exactly these phases; the AOT path (§3.2(b)) has to redo what dotty's transform phases do. This is the single largest chunk of "compiler" work in the project [I].
- **Inlining is not done in TASTy** [V-src]: `Inlines.needsInlining` returns true in `Typer` only for transparent inline calls (`ctx.phase == Phases.typerPhase && needsTransparentInlining(tree)`), otherwise only in `Phases.inliningPhase`, which is after `pickler`. The `Inlining` phase doc says "Inlines all calls to inline methods that are not in an inline method or a quote" [V-src]. Therefore:
  - Ordinary `inline def` calls (e.g. much of `scala.compiletime`, `inline` helpers in collections and user libraries) appear as *calls* in TASTy. The interpreter must expand them itself: substitute the body with `inline` parameters handled by-value/by-name per spec, evaluate `inline if`/`inline match`/`constValue`/`erasedValue`/`summonInline` at "interpretation time". Most of these can be evaluated dynamically with the same observable result (they reduce to constants given static types available in TASTy) [I], but `erasedValue`/type-level matches require type-level evaluation the interpreter must implement from the TASTy types [I].
  - **Macros** (`inline def f = ${ impl }`) cannot be executed without the compiler's quotes/reflection API. A Rust interpreter cannot expand them. Options: (1) declare macros unsupported in interpreted user code (library code that was compiled with scalac has *its* macro call sites already... no — see above: non-transparent macro call sites *inside* library code are also unexpanded in that library's TASTy, but the library's *class files* are post-expansion). (2) Provide an optional JVM-based pre-pass (dotc `inlining` + a custom pickle of post-inlining trees, or `-Yretain-trees`) producing "expanded TASTy" for the interpreter — this makes the toolchain depend on a JVM at *build* time of a script, which conflicts with the no-JVM goal for end users but might be acceptable for library preparation [I]. (3) Fall back to the class-file path (§6) for macro-heavy libraries. This needs an explicit product decision.
- **INLINED nodes exist** (`INLINED Length expr_Term call_Term? ValOrDefDef*`) for the transparent-inline expansions that *are* in TASTy [V-src]; the interpreter should treat them as blocks.

---

## 5. Rust vs C++ for this project

| Concern | Rust | C++ | Assessment |
|---|---|---|---|
| Interpreter core (IR, closures, values, loader) | Excellent: enums + pattern matching, `Box<dyn Fn>`/fn pointers, slices; borrow checker helps with IR ownership | Fine | Rust |
| GC heap with raw layout, conservative stack scan, RTTI pointers | Necessarily `unsafe` in Rust; borrow checker gives nothing here. Mitigation: raw `*mut Object` everywhere in the runtime, a small `unsafe` core, non-moving heap so no rooting API is needed (the Servo/`gc-arena`-style rooting pain only appears with moving or "exact stack" designs) [V][I] | Native territory; UB risks identical | Tie; Rust acceptable if we accept a moving-GC-free design initially |
| Reusing Scala Native's C/C++ runtime (Immix, `eh.c`/`eh.cpp`, `delimcc.c`) | FFI via `cc`/`bindgen` is routine; unwinding across Rust frames needs `extern "C-unwind"` and care [I] | Direct | Slight C++ advantage, but only relevant for option 3(a) |
| Cranelift / AOT backend | Cranelift is Rust-only; first-class [V] | Would need LLVM (C++) or asmjit (as BeamAsm does) [V] | Rust |
| MMTk | Rust-native; C++ bindings exist (OpenJDK) but the binding side is written in Rust anyway [V] | Via FFI | Rust |
| LLVM interop | `inkwell`/`llvm-sys` fine but heavy [V] | Native | C++ only if LLVM is central — it is not in the recommended design |
| Class-file/zip/libffi/tree-sitter crates | all available and maintained [V] | libraries exist too | Tie |
| Precedents | Wasmtime/Cranelift, Wasmi, RustPython, Boa, Ristretto, piccolo, Ruby's and Julia's MMTk bindings are all Rust VMs/runtimes [V] | HotSpot, V8, Dart, LuaJIT, CPython (C) | Both viable; Rust has enough successful VM precedent now |
| Team/tooling | Rust build is simpler for a multi-platform CLI (macOS/Linux/Windows) [I] | CMake/toolchain variance | Rust |

**Recommendation: Rust**, with the explicit design rule that the runtime heap is a raw, non-moving arena behind an `unsafe` module; do not fight the borrow checker by trying to express GC references as safe Rust types (`gc-arena`-style branding) in the hot interpreter. If option 3(a) is ever pursued, the Scala Native runtime pieces are consumed as C libraries through FFI; no C++ code needs to be written.

---

## 6. Executing JVM bytecode instead of (or in addition to) TASTy

What it would take, honestly:

1. **A JVM interpreter** — class loading, verification (or trust), constant pool resolution, `invokedynamic` with the `LambdaMetafactory` and `StringConcatFactory` bootstraps (Scala emits `invokedynamic` for lambdas since 2.12; Scala 3's backend does the same [V]; string concatenation via `StringConcatFactory` on Java 9+ targets [I]), `MethodHandle`s at least for those two bootstraps, monitors, `Class`/reflection basics, `Unsafe`/`VarHandle` usage inside the JDK (the JDK's own collections/`String`/`ConcurrentHashMap` rely on them). Ristretto shows this is doable in Rust with a hand-implemented native surface that throws `UnsatisfiedLinkError` for anything missing [V].
2. **A class library.** Two choices:
   - *OpenJDK's.* Licence GPLv2 + Classpath Exception: linking/using it with proprietary code is explicitly permitted; modifications to the library itself are GPL [V]. Size: a jlink image with `java.base` only is on the order of 30–50 MB (reported "hello world ~30 MB", "as small as 53 MB" with a few modules) [V]; the class data alone (`lib/modules`) is a large fraction of that. Startup would require our own class-data cache/snapshot (the JVM needs CDS/AppCDS for the same reason) [I]. Every `native` method (`System`, `Thread`, `FileDescriptor`, NIO, `Unsafe`, `Reflection`, ...) must be reimplemented in Rust (Ristretto's approach) or loaded from `libjava.so` with a compatible JNI (Espresso's approach: substitutions plus NFI to genuine `libjava`) [V].
   - *Scala Native's javalib compiled to class files.* Scala Native's javalib is "a subset of the JDK core libraries reimplemented in Scala" [V], but it is written against Scala Native primitives (`scala.scalanative.unsafe`, `runtime.Intrinsics`, `ffi`, RE2 via native binding for `java.util.regex`) [V][V-src], so it is not JVM-compilable as-is; it would need a port (replace intrinsics with our interpreter's intrinsics) — at which point we are effectively at the TASTy path anyway [I].
3. **Scala library on the JVM path** additionally needs `scala.runtime.*` support classes and the `scala3-library` JAR's class files (which *are* post-inlining/post-erasure — the one real advantage: macros already expanded) [V-src][I].

**Assessment.** A JVM-bytecode execution path is a second, larger project (bytecode verifier semantics, `invokedynamic`, `Unsafe`, JDK natives) and drags in either the JDK (size, startup) or a javalib port. Its unique benefits are (1) running arbitrary pre-existing JARs and (2) sidestepping the macro/inlining problem because class files are post-expansion. Given the stated target (Scala-Native-compatible subset, fast startup, no JVM), the recommendation is:

- **Primary: TASTy → LIR** with a Scala-Native-like runtime library set (write our own `javalib` subset in Scala against *our* intrinsics, or port Scala Native's — the latter is the fastest route to `java.lang`/`java.util` coverage [I]).
- **Class files are read for symbols only** (Java signatures of dependencies), using `cafebabe`/`noak`.
- Revisit bytecode execution only if "run this existing JVM library" becomes a business requirement; Ristretto (Apache-2.0/MIT) would then be the thing to evaluate embedding or forking rather than starting from zero [V].

---

## 7. Proposed architecture and phased plan

```
             ┌──────────────┐    ┌───────────────┐    ┌────────────────────┐
 .tasty/.jar │ TASTy reader │──▶ │ Linker/Lowerer│──▶ │ LIR (resolved,     │
 (coursier)  │ (Rust)       │    │ erasure, PM,  │    │ per-method, lazy)  │
             └──────────────┘    │ closures,     │    └───────┬────────────┘
             ┌──────────────┐    │ inline expand │            │
 .class      │ classfile    │──▶ │ (symbols only │   ┌────────▼─────────┐   ┌──────────────┐
 (Java deps) │ (cafebabe)   │    │  from .class) │   │ Closure compiler │   │ Cranelift    │
             └──────────────┘    └───────────────┘   │ + inline caches  │   │ baseline tier│
                                                     └────────┬─────────┘   │ (phase 3)    │
                                                              │             └──────┬───────┘
                                        ┌─────────────────────▼────────────────────▼───────┐
                                        │ Runtime: Scala-Native-shaped heap (rtti/vtable/  │
                                        │ itable/arrays/UTF-16 strings), non-moving GC     │
                                        │ (Boehm MVP → own Immix-like / MMTk), intrinsics, │
                                        │ libffi for @extern, snapshot load/store          │
                                        └──────────────────────────────────────────────────┘
```

- **Phase 0 (spike, weeks):** Rust TASTy unpickler for `scala3-library` 3.9.0; count what tree/type tags appear; prototype LIR for a "hello world + collections" slice; Boehm GC; measure load time of the stdlib TASTy (this number decides how aggressively lazy loading and snapshotting are needed).
- **Phase 1:** closure interpreter, object model per §2.2, exceptions as `Result`, `java.lang`/`java.util` subset (port from Scala Native javalib where licence-compatible — Scala Native is Apache-2.0 [I: check]), `@tailrec` loops, inline-method expansion (no macros).
- **Phase 2:** heap + metadata snapshot of the initialised stdlib; on-disk LIR cache; quickening/inline caches; own GC.
- **Phase 3:** Cranelift baseline tier sharing the runtime; optional AOT of the stdlib to object files embedded in the snapshot.
- **Deferred/experimental:** linking a Scala Native `libraryStatic` build; JVM bytecode execution; multithreading (lock word already reserved).

---

## 8. Open questions to settle with spikes

1. **Macro policy** (§4.4): unsupported vs JVM pre-expansion. Quantify how many `scala3-library` and typical script-dependency call sites are non-transparent inline / macro.
2. **Stdlib load latency**: time to unpickle all of `scala3-library` TASTy in Rust (expected tens of ms if done lazily per class; hundreds if eager) [I]; this fixes the snapshot priority.
3. **`Value` representation**: 16-byte tagged enum vs 8-byte NaN-boxing/pointer tagging; measure on the closure interpreter before committing (affects every closure signature).
4. **Conservative stack scanning in Rust**: verify that a register-spill + stack-bounds scan is robust under Rust optimisations (derived pointers into the middle of objects — Scala Native's `Bytemap_isPtrAligned` accepts only object starts [V-src]; interior pointers from slices would be missed, so `Value`s must hold object base pointers only) [I].
5. **Scala Native ABI compatibility depth**: confirm class-id assignment and itable hashing rules in `Metadata.scala`/`ITable.scala` if option 3(a) is ever attempted [U].

---

## 9. Sources

Interpreter performance and design
- Titzer, "A fast in-place interpreter for WebAssembly" (OOPSLA 2022): https://arxiv.org/abs/2205.01183 ; https://dl.acm.org/doi/abs/10.1145/3563311
- Larose, Kaleba, Burchell, Marr, "AST vs. Bytecode: Interpreters in the Age of Meta-Compilation" (OOPSLA 2023): https://stefan-marr.de/2023/10/ast-vs-bytecode-interpreters/ ; https://dl.acm.org/doi/10.1145/3622808
- Neil Mitchell, "Writing a fast interpreter" (Rust; AST vs closures vs bytecode): http://neilmitchell.blogspot.com/2020/04/writing-fast-interpreter.html ; slides https://ndmitchell.com/downloads/slides-cheaply_writing_a_fast_interpreter-23_feb_2021.pdf
- lamedh issue #200 (closure compilation vs bytecode, anecdotal): https://github.com/pnathan/lamedh/issues/200
- Wasmi v0.32 execution-engine redesign (register bytecode, lazy translation, Coremark numbers): https://wasmi-labs.github.io/blog/posts/wasmi-v0.32/
- "Less Is More: Merging AST Nodes To Optimize Interpreters" (supernodes, Truffle): https://kar.kent.ac.uk/93936/
- What's New in Python 3.11 (Faster CPython numbers): https://docs.python.org/3/whatsnew/3.11.html ; PEP 659: https://peps.python.org/pep-0659/
- PEP 744 (CPython copy-and-patch JIT): https://peps.python.org/pep-0744/ ; Xu & Kjolstad, "Copy-and-Patch Compilation": https://arxiv.org/abs/2011.13127
- V8 "Firing up the Ignition interpreter": https://v8.dev/blog/ignition-interpreter ; V8 Ignition docs: https://v8.dev/docs/ignition
- V8 "Sparkplug — a non-optimizing JavaScript compiler": https://v8.dev/blog/sparkplug
- Wasmtime baseline compilation RFC (Winch; 15–20× faster compile, 1.1–1.5× slower code): https://github.com/bytecodealliance/rfcs/blob/main/accepted/wasmtime-baseline-compilation.md ; Winch README: https://github.com/bytecodealliance/wasmtime/blob/main/winch/README.md
- Wasmtime 1.0 performance (Cranelift vs LLVM/WAVM/V8): https://bytecodealliance.org/articles/wasmtime-10-performance ; Cranelift vs LLVM notes: https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/docs/compare-llvm.md
- Deegen / "Building the fastest Lua interpreter.. automatically!": https://sillycross.github.io/2022/11/22/2022-11-22/
- Erlang "BeamAsm, the Erlang JIT": https://www.erlang.org/doc/apps/erts/beamasm.html ; "The Road to the JIT": https://www.erlang.org/blog/the-road-to-the-jit/
- GraalVM Espresso: https://www.graalvm.org/jdk23/reference-manual/espresso/ ; "How Espresso works": https://github.com/oracle/graal/blob/master/espresso/docs/how-espresso-works.md
- Truffle: https://www.graalvm.org/latest/graalvm-as-a-platform/language-implementation-framework/

Scala Native (docs and source on `main`)
- Runtime / GC settings (Immix "mostly-precise", Commix, Boehm, None): https://scala-native.org/en/latest/user/runtime.html
- NIR: https://scala-native.org/en/latest/contrib/nir.html
- sbt build targets (`libraryDynamic`, `libraryStatic`, `@exported`): https://scala-native.org/en/latest/user/sbt.html ; interop (`@exported`, `ScalaNativeInit`, `CFuncPtr`): https://www.scala-native.org/en/latest/user/interop.html ; dylib PR: https://github.com/scala-native/scala-native/pull/2145
- 0.5.7 changelog (class-specific itables): https://scala-native.org/en/latest/changelog/0.5.x/0.5.7.html
- javalib overview: https://scala-native.org/en/latest/lib/javalib.html
- `CommonMemoryLayouts.scala` (object/array header, RTTI, itable layout): https://github.com/scala-native/scala-native/blob/main/tools/src/main/scala/scala/scalanative/codegen/CommonMemoryLayouts.scala
- `RuntimeTypeInformation.scala`, `MemoryLayout.scala`, `ITable.scala`, `DynamicHashMap.scala`: https://github.com/scala-native/scala-native/tree/main/tools/src/main/scala/scala/scalanative/codegen
- `runtime/Object.scala` (getClass = load rtti; address hash; monitor): https://github.com/scala-native/scala-native/blob/main/nativelib/src/main/scala/scala/scalanative/runtime/Object.scala
- `java/lang/String.scala` (`_String`: `Array[Char]`, offset, count, cachedHashCode): https://github.com/scala-native/scala-native/blob/main/javalib/src/main/scala/java/lang/String.scala
- `eh.c` / `eh.cpp` (exception unwinding), `dylib_init.c`, `delimcc.c`, `itable_search.c`, `dyndispatch.c`: https://github.com/scala-native/scala-native/tree/main/nativelib/src/main/resources/scala-native
- Immix `Marker.c` (conservative range marking, precise field marking), `GCRoots.c`, safepoint trampolines: https://github.com/scala-native/scala-native/tree/main/nativelib/src/main/resources/scala-native/gc
- Scala Native Immix staging repo: https://github.com/scala-native/immix

Scala 3 / TASTy
- `TastyFormat.scala` (versions, `INLINED`, `LAMBDA`, sections): https://github.com/scala/scala3/blob/main/tasty/src/dotty/tools/tasty/TastyFormat.scala
- Compiler phases (pickler before inlining/erasure): https://www.scala-lang.org/api/3.4.2/docs/docs/contributing/architecture/phases.html
- `Inlines.needsInlining` (typer expands only transparent inlines): https://github.com/scala/scala3/blob/main/compiler/src/dotty/tools/dotc/inlines/Inlines.scala ; `transform/Inlining.scala`: https://github.com/scala/scala3/blob/main/compiler/src/dotty/tools/dotc/transform/Inlining.scala ; `Typer.scala` adapt: https://github.com/scala/scala3/blob/main/compiler/src/dotty/tools/dotc/typer/Typer.scala
- tasty-query: https://github.com/scalacenter/tasty-query
- shardulc/tasty-interpreter (Scala reference interpreter; details unverified): https://github.com/shardulc/tasty-interpreter
- Scala 2.12 indylambda / invokedynamic: https://www.scala-lang.org/news/2.12.0/ ; https://gist.github.com/retronym/0178c212e4bacffed568

GC in Rust
- gc-arena README / design ("mutation xor collection"): https://github.com/kyren/gc-arena ; "Techniques for Safe Garbage Collection in Rust": https://kyju.org/blog/rust-safe-garbage-collection/
- piccolo (stackless Lua VM on gc-arena): https://github.com/kyren/piccolo ; https://kyju.org/blog/piccolo-a-stackless-lua-interpreter/
- Manish Goregaokar, "A Tour of Safe Tracing GC Designs in Rust": https://manishearth.github.io/blog/2021/04/05/a-tour-of-safe-tracing-gc-designs-in-rust/
- rust-gc (`gc` crate): https://crates.io/crates/gc ; boa_gc: https://docs.rs/boa_gc ; dumpster: https://lib.rs/crates/dumpster ; shredder: https://crates.io/crates/shredder ; safe-gc: https://crates.io/crates/safe-gc
- bdwgc-alloc (Boehm `GlobalAlloc`): https://github.com/raviqqe/bdwgc-alloc ; boehm_gc crate: https://docs.rs/boehm_gc/latest/boehm_gc/
- MMTk: https://www.mmtk.io/code ; mmtk-core: https://github.com/mmtk/mmtk-core ; porting guide (NoGC, next steps): https://docs.mmtk.io/portingguide/howto/nogc.html ; https://docs.mmtk.io/portingguide/howto/next_steps.html ; Julia binding: https://github.com/mmtk/mmtk-julia ; OpenJDK binding: https://github.com/mmtk/mmtk-openjdk
- Ruby 3.4 modular GC + MMTk: https://railsatscale.com/2025-01-08-new-for-ruby-3-4-modular-garbage-collectors-and-mmtk/ ; Kunshan Wang, "Reworking Memory Management in CRuby: A Practitioner Report" (conservative stack scanning with MMTk): https://railsatscale.com/2025-09-16-reworking-memory-management-in-cruby/paper.pdf
- RustPython `PyObjectRef` (Rc-based, cycles): https://rustpython.github.io/website/rustpython_vm/pyobject/type.PyObjectRef.html ; https://github.com/RustPython/RustPython/issues/371

Snapshots / mixed-mode systems
- GraalVM Native Image memory management & image heap: https://www.graalvm.org/latest/reference-manual/native-image/optimizations-and-performance/MemoryManagement/ ; InfoQ overview: https://www.infoq.com/articles/native-java-graalvm/
- Dart VM internals (kernel, AppJIT, AOT snapshots, clustered serialisation): https://mrale.ph/dartvm/ ; Dart snapshots wiki: https://github.com/dart-lang/sdk/wiki/Snapshots
- Julia PackageCompiler sysimages: https://julialang.github.io/PackageCompiler.jl/stable/sysimages.html
- Emacs portable dumper: https://lwn.net/Articles/707619/ ; https://github.com/emacs-mirror/emacs/blob/master/src/pdumper.h
- OCaml native vs bytecode (no mixing): https://ocaml.org/manual/5.0/native.html
- PyPy "Inside cpyext: Why emulating CPython C API is so hard": https://pypy.org/posts/2018/09/inside-cpyext-why-emulating-cpython-c-8083064623681286567.html

Rust building blocks
- tree-sitter-scala: https://github.com/tree-sitter/tree-sitter-scala ; eed3si9n on Scala 3 parsing: https://eed3si9n.com/fast-scala3-parsing-with-tree-sitter/
- cafebabe: https://crates.io/crates/cafebabe ; noak: https://lib.rs/crates/noak ; classfile-parser: https://github.com/Palmr/classfile-parser ; mokapot: https://docs.rs/mokapot
- zip: https://crates.io/crates/zip ; rc-zip: https://github.com/bearcove/rc-zip ; rawzip: https://lib.rs/crates/rawzip
- cranelift-jit: https://crates.io/crates/cranelift-jit ; Cranelift: https://cranelift.dev/
- inkwell: https://crates.io/crates/inkwell ; llvm-sys: https://crates.io/crates/llvm-sys
- libffi crate: https://crates.io/crates/libffi
- Ristretto JVM (Rust): https://github.com/theseus-rs/ristretto ; ristretto_vm docs: https://docs.rs/ristretto_vm/latest/ristretto_vm/
- "tasty" crate on crates.io (unrelated TOML API tester; confirms no Rust TASTy reader): https://crates.io/crates/tasty

JVM class library / licensing / size
- OpenJDK GPLv2 + Classpath Exception: https://openjdk.org/legal/gplv2+ce.html ; Azul on CPE: https://www.azul.com/blog/why-the-classpath-exception-is-so-important/
- jlink image sizes (java.base-only ≈ 30–50 MB reports): https://snyk.io/blog/jlink-create-docker-images-spring-boot-java/ ; https://stevenpg.com/posts/jlink-java-runtime-optimization/
- JVM/Scala startup baseline: https://www.scala-lang.org/old/node/3408.html ; scala-cli instant-startup scripts (Scala Native): https://scala-cli.virtuslab.org/docs/cookbooks/introduction/instant-startup-scala-scripts/
