# 04 — Garbage collector options for the NIR VM

Date: 2026-09-25 · Status: research for [ADR 0004](../adr/0004-immix-garbage-collector.md)

Claims are marked **[V]** (verified: read in the checkout, a fetched page or a crate's docs) or
**[I]** (inferred). Sources are listed at the end; `ref/…` paths refer to the Scala Native
v0.5.12 checkout, `crates/…` to this repository.

## TL;DR

- **Recommendation: compile Scala Native's own Immix collector (C, Apache-2.0, ~4.3k lines in
  `gc/immix`, `gc/immix_commix`, `gc/shared`) into `libsnrt` and drive it from the VM.** It is
  written for exactly our object layout and RTTI, is non-moving, traces the heap precisely
  through `refFieldOffsets`, scans roots conservatively, accepts extra root ranges through
  `scalanative_GC_add_roots(low, high)` (which is how our Rust-heap frame slots become roots),
  handles weak references, and already has the single-threaded and OS-threaded modes we need
  later. Estimated effort: 6–10 days, most of it on the VM side (writing `refFieldOffsets` into
  RTTI, one contiguous root-registered slot stack instead of `Vec<Vec<u64>>` frames, root
  registration of VM tables).
- **Runner-up: Boehm (bdwgc)** through Scala Native's `gc/boehm/gc.c` shim plus a vendored or
  system `libgc`. 4–7 days, fewer VM changes, but a fully conservative collector whose GC
  overhead Scala Native measured at roughly 10× Immix's on throughput benchmarks, and a build
  dependency (autotools/CMake or `brew`/`apt`).
- **The "Node.js GC" is almost certainly V8's Oilpan/cppgc.** It is a C++ library that requires
  objects to inherit `GarbageCollected<T>` and to be traced through `Member<T>` and `Trace()`;
  its standalone GitHub export was archived in November 2025. It cannot manage objects with
  Scala Native's layout. Not an option.
- **MMTk** is the serious long-term alternative (Rust, MIT/Apache, MarkSweep and non-moving
  Immix, pinning, valid-object bits for conservative roots), but the binding is 3–6 weeks of
  work, and the porting guide itself warns not to underestimate even a NoGC port.
- **Rust GC crates** (gc-arena, dumpster, rust-gc) manage Rust values, not raw memory in a
  foreign layout; none applies.
- **ADR 0004 changes:** keep its requirements (non-moving, precise heap, conservative frame
  roots), but replace "a mark-sweep collector of our own" with "Scala Native's Immix, compiled
  into `libsnrt`", and add the VM-side prerequisites listed in §3.4.

## 0. What the collector must fit

Facts about the VM, verified in the code:

- Objects use Scala Native's layout: an 8-byte header holding the RTTI pointer, arrays have
  `length` at offset 8 and `stride` at 12 (`crates/vm/src/layout.rs`) [V]. This is Scala
  Native's *single-threaded* layout; with `SCALANATIVE_MULTITHREADING_ENABLED` Scala Native adds
  a lock word to every header (`USES_LOCKWORD`, `ObjectHeader.h`) [V].
- Allocation is a bump allocator over 1 MiB chunks from Rust's global allocator that never
  frees (`crates/vm/src/heap.rs`); `scalanative_GC_*` externs are answered in Rust and
  `collect`, `add_roots`, `init` and friends are no-ops (`crates/vm/src/natives/gc.rs`) [V].
- Frame slots are untyped `u64` in `Vec<u64>`s recycled through `Vec<Vec<u64>>`
  (`crates/vm/src/interp/mod.rs`, `exec.rs`, `invoke.rs`); they live on the Rust heap, so no
  stack-scanning collector sees them [V].
- RTTI objects are `java.lang.Class` instances laid out as Scala Native expects; the VM fills
  `id`, `size`, `idRangeUntil`, `superClass`, `interfacesCount`, `interfaces` and `name`, but
  **not** `refFieldOffsets`, `itablesCount` or `itables` (`crates/vm/src/rtti.rs`) [V]. The
  per-field types needed to compute reference offsets are known (`class/fields.rs` keeps
  `Field { offset, ty }`) [V].
- Class ids: primitives 0–10, `Object` 11, `Array` 12, the ten array classes 13–22, other
  classes counting up from 23, traits counting down from 0 (`rtti.rs::class_id_value`) [V].
  Scala Native assigns ids so that subclasses form contiguous ranges; we do not [V].
- `libsnrt` is Scala Native's C runtime (72 `.c` files, no `gc/` directory, no `StackTrace.c`)
  compiled by `crates/snrt/build.rs` with `-DSCALANATIVE_COMPILE_ALWAYS` into one shared
  library, on macOS with `-undefined dynamic_lookup` [V].
- Targets: macOS arm64 and Linux x86-64; green threads next (ADR 0003), OS threads later.

Requirements, from ADR 0004 and the above: non-moving (addresses are handed to C); tracing
that understands our layout; roots from Rust-heap frame slots, VM tables and the Rust stack
(the interpreter holds object addresses in Rust locals while it runs); embeddable from Rust on
both targets; a path to green and later OS threads.

## 1. "The GC the Node.js/V8 team released": Oilpan / cppgc

**Identification.** No 2026 release of a new garbage collector by the Node.js project or the
V8 team could be found (searches for Node.js/V8 GC announcements in 2026 return only tutorials
and the Node Congress 2026 talk "Garbage Collection Between V8, cppgc (Oilpan), and Native
Runtimes") [V]. What matches the description is **Oilpan**, V8's C++ garbage collector,
shipped as the `cppgc` library since V8 9.4 (announced on v8.dev, 2021) [V]. Node.js exposes it
to C++ addons ("experimental support for cppgc (Oilpan)", headers `<cppgc/*>` bundled since
Node 20.6; ABI stability not guaranteed across minor releases) [V]. The 2026 talk is what
probably made it look like a recent release [I].

- **Name/repo/license/language.** `cppgc`, C++, in the V8 tree (`v8/include/cppgc`,
  BSD-3-Clause, "the V8 project authors") [V]. The standalone export `oilpan-gc/cppgc` (Bazel
  build, synced by Copybara) was **archived on 2025-11-14** and is read-only [V]. Rust access
  exists only through Deno's `v8` crate (`v8::cppgc`, version 152.2.0 of 2026-08-20), which
  links all of V8 [V].
- **API.** Objects derive from `GarbageCollected<T>`, are created with
  `MakeGarbageCollected<T>(heap.GetAllocationHandle(), …)`, hold on-heap pointers in
  `Member<T>`/`WeakMember<T>`, off-heap in `Persistent<T>`, and implement `Trace(Visitor*)`;
  a `cppgc::Heap` is created from a `cppgc::Platform` [V]. Mark-and-sweep, precise on the heap,
  **conservative on the native stack** when a GC happens with a non-empty stack, precise at
  event-loop boundaries; "limited compaction" for selected spaces (i.e. not strictly
  non-moving, but compaction is opt-in per space) [V]. Heaps are thread-local: objects are
  allocated and reclaimed on the thread that allocated them [V].
- **Fit.** Tracing is dispatched by C++ type (`TraceTrait<T>`), and a `Member<T>` is cppgc's own
  pointer representation, so our objects (raw layout, RTTI-described fields, object arrays of
  raw pointers) cannot be cppgc objects without a C++ type per class and a change of field
  representation [V for the API, I for the consequence]. Building it means building V8 (Bazel
  export, archived) or pulling ~100 MB of prebuilt V8 through the `v8` crate. **Rejected.**

## 2. Boehm-Demers-Weiser GC (bdwgc)

- **What/license/platforms.** Conservative mark-sweep for C/C++, MIT-style licence (Homebrew
  lists "Boehm-GC AND HPND-sell-variant"), latest release 8.2.12 (2025-02-05), 8.0.x still
  maintained (a "Sep 2026" 8-0 branch update PR exists) [V]. Builds with CMake (recommended),
  autotools, `Makefile.direct` or Zig; threads on by default (`GC_THREADS`), macOS uses Mach
  thread suspension (`darwin_stop_world.c`); Homebrew ships arm64 bottles [V]. **Non-moving**
  [V]. Scala Native still ships a Boehm backend and the docs still list it [V].
- **From Rust.** `bdwgc-alloc` 0.6.14 (2026-09-25, MIT) vendors bdwgc as a git submodule,
  builds it with autotools by default or with the `cmake` feature, and exposes a `GlobalAlloc`;
  its docs.rs build has failed since 0.6.3 because the build needs the toolchain [V]. `boehm_gc`
  is a 2016 prototype (0.0.1) [V]. For us the simplest route is not a crate but the same thing
  `crates/snrt/build.rs` already does: compile a vendored bdwgc (CMake via the `cmake` crate, or
  its single-file `extra/gc.c` amalgamation) or link the system `libgc` (`brew install bdw-gc`,
  `apt install libgc-dev`) and use Scala Native's `gc/boehm/gc.c` shim, which implements the
  whole `scalanative_GC_*` interface on Boehm including weak references through
  `GC_general_register_disappearing_link` and finalizers, and `add_roots` through
  `GC_add_roots` [V]. Vendoring costs a C toolchain build step on every clean build [I].
- **Conservative vs precise.** `GC_malloc` is scanned conservatively, `GC_malloc_atomic` not at
  all; `gc_typed.h` adds typed allocation: `GC_make_descriptor(bitmap, nbits)` from a bitmap of
  pointer-holding words, `GC_malloc_explicitly_typed(size, d)` and `GC_calloc_explicitly_typed`
  for arrays; words beyond the bitmap are assumed pointer-free; typed objects cannot be
  `GC_realloc`ed [V]. Our `refFieldOffsets` maps directly onto such a bitmap, and object arrays
  onto `GC_calloc_explicitly_typed`, so the heap could be traced precisely; the stack, data
  segments and registered roots stay conservative [I]. The Scala Native shim uses only the
  conservative `GC_malloc` (atomic for primitive arrays) [V].
- **Roots.** Boehm scans the main stack from `GC_stackbottom` (found at `GC_INIT`), registered
  threads' stacks, the data segments of the executable and all loaded dynamic libraries unless
  `GC_set_no_dls(1)`, and explicit ranges from `GC_add_roots(low, high)`/`GC_remove_roots`
  ("wizards only") [V]. Our frame slots would be registered as root ranges; memory from Rust's
  allocator is otherwise invisible to it, which is correct for us (nothing else in Rust memory
  should keep objects alive) [I]. VM-owned metadata (RTTI, statics) can use
  `GC_malloc_uncollectable` (scanned, never freed) [V].
- **Threads.** Signal/Mach-based stop-the-world; threads created through `GC_pthread_create` or
  registered with `GC_register_my_thread` after `GC_allow_register_threads()` [V]. Green threads
  on one OS thread need nothing extra as long as every green thread's slot stack is a registered
  root range and its native stack (if stackful) is registered as well [I]. OS threads later need
  only registration [V].
- **Performance.** Scala Native, when it made Immix the default (0.3.8), reported "on all
  throughput benchmarks Immix performs much better than Boehm… average GC overhead went down
  from 100% to 10%, with some benchmarks seeing close to 5× improvements" [V]. Boehm's allocation
  is free-list based and takes a lock in threaded builds; incremental mode exists but is "not
  advisable unless dirty bits are available" [V].
- **Effort.** Build (vendored CMake or system lib) 1 day; `scalanative_GC_alloc*` routed to the
  C shim 0.5 day; frame-slot root ranges (same VM refactor as in §3.4) 1–2 days; VM tables as
  roots or uncollectable 1 day; weak references already handled by the shim; testing 1–2 days.
  **4–7 days.** Adding typed descriptors for a precise heap: +1–2 days.
- **Risks.** Conservative everything: a heap full of `u64` slots and UTF-16 strings produces
  false retention; scanning all data segments of a Rust binary costs time per collection
  (`GC_set_no_dls` mitigates, then every root must be registered by hand) [I]. Maintenance
  risk is low (30 years, active) [V].

## 3. Scala Native's collectors as C sources (`nativelib` jar, v0.5.12)

The `gc/` tree in the checkout holds `none`, `boehm` (shim), `immix`, `commix` (parallel and
concurrent Immix), `immix_commix` (shared marker helpers, `GCRoots`, `ObjectHeader.h`) and
`shared` (memory mapping, parsing, safepoint trampolines, JMX stats) [V]. Scala Native compiles
one of them with `-DSCALANATIVE_GC_<NAME>` (`build/NativeLib.scala`) and, for threads,
`-DSCALANATIVE_MULTITHREADING_ENABLED` (`build/LLVM.scala`); Immix, `immix_commix` and `shared`
together are ~4.3k lines of C and two `.S` files [V]. Licence: Apache-2.0
(`native/LICENSE-scala-native.md`) [V].

### 3.1 How Immix works (from the sources)

- **Heap.** One `mmap`ed region of blocks (32 KiB, lines of 256 B) plus a large-object space,
  a `Bytemap` with one byte of state per 16-byte allocation unit (free / placeholder /
  allocated / marked), block and line metadata; `Heap_Init(min, max)` reserves the maximum up
  front, `Heap_Grow` commits more [V]. `GC_INITIAL_HEAP_SIZE`, `GC_MAXIMUM_HEAP_SIZE`,
  `GC_STATS_FILE`, `GC_LOG_FILE` control it [V].
- **Allocation.** Bump allocation in a thread's `Allocator` (small objects) or
  `LargeAllocator`; when a block cannot be found, `Heap_Collect` runs, then `Heap_Grow`, then
  `Heap_exitWithOutOfMemory` (which calls `StackTrace_PrintStackTrace`, so we need that symbol
  or a stub) [V]. `scalanative_GC_alloc(rtti, size)` rounds to 16, allocates, zeroes and writes
  the header; `scalanative_GC_alloc_array` also writes `length` and `stride` [V].
- **Marking, precise on the heap.** `Marker_Mark` pops objects and, for a plain object, walks
  `rtti->refFieldOffsets` (an `int32_t` array terminated by −1) and marks each field; for
  `__object_array_id` it marks every element; for `__blob_array_id` it scans the prefix
  conservatively; for `__boxed_ptr_id` it treats the single field conservatively; a weak
  reference's referent field (`__weak_ref_field_offset`, classes in
  `__weak_ref_ids_min..max`) is skipped and nullified later by `WeakReferences_Nullify`, which
  then invokes the callback set by `scalanative_GC_set_weak_references_collected_callback` [V].
  Marking never moves objects; Immix here is a **non-moving** mark-region collector ("mostly
  precise" in Scala Native's own words) [V].
- **Roots, conservative.** `Marker_MarkRoots` scans (1) each mutator thread's native stack
  between `stackBottom` (address of a local captured in `scalanative_GC_init`) and `stackTop`
  (captured with the registers by `MutatorThread_switchState` at the start of a collection),
  (2) the module array `__modules[0..__modules_size)`, (3) every range registered through
  `scalanative_GC_add_roots`/`GC_Roots_Add` (a mutex-protected linked list of `[low, high)`
  ranges, scanned word by word), then drains the mark stack [V]. Any word that lands inside the
  heap is resolved to an object start, **interior pointers included**
  (`Object_GetUnmarkedObject` → `Object_getInnerPointer`) [V]. Words that point outside the
  heap are ignored, so metadata we keep in ordinary memory does no harm [V].
- **Sweeping.** Lazy, per block/line (`Heap_Recycle`); large objects are freed by their
  superblock [V].
- **Threads.** Single-threaded builds stop nothing; with `SCALANATIVE_MULTITHREADING_ENABLED`
  Immix registers threads through `scalanative_GC_pthread_create`, stops the world with
  cooperative safepoints (`scalanative_GC_yield`, polled by generated code, or trap-based yield
  points via `SafepointPollTrampoline-*.S`/`YieldPointTrap.c`), and expects threads to switch to
  `Unmanaged` around blocking C calls (`scalanative_GC_set_mutator_thread_state`) [V]. That
  build also puts a lock word into every object header and `Rtti` (`USES_LOCKWORD`) [V].

### 3.2 What Immix needs from the runtime

| Need | Provided by Scala Native's linker | We would provide |
|---|---|---|
| `Rtti` struct: `cls, id, interfacesCount, interfaces, name, size, idRangeUntil, refFieldOffsets, itableCount, itable, superclass` | `RuntimeTypeInformation.scala` | Our RTTI already has this layout; **`refFieldOffsets` is missing** (§0) |
| `__object_array_id`, `__blob_array_id`, `__array_ids_min/max`, `__boxed_ptr_id`, `__weak_ref_ids_min/max`, `__weak_ref_field_offset` (`extern const int`) | `Generate.scala` | A C file with definitions plus a setter called after bootstrap [I] |
| `__modules`, `__modules_size` | `Generate.scala` | Define `__modules_size = 0` and register the VM's module table as a root range instead [I] |
| `scalanative_GC_init` called from the outermost frame | `main` | Call it at the top of `run_main`; frames deeper on the Rust stack are then scanned [V] |
| `StackTrace_PrintStackTrace` | `StackTrace.c` (needs `platform/unwind.h`) | Compile it or stub it [V] |
| Safepoints / `scalanative_GC_yield` | generated code | Only for OS threads; a no-op until then [V] |

Our class ids do not form subclass ranges, so `__weak_ref_ids_min..max` needs either a reserved
contiguous id range for `java.lang.ref.WeakReference` and its subclasses (Scala Native's
`__weak_ref_ids` are exactly that) or an empty range, in which case weak references are simply
strong until fixed [I]. `idRangeUntil` and itables are not used by the collector [V].

### 3.3 Roots from our frame slots

`scalanative_GC_add_roots(low, high)` accepts any address range, so Rust-heap memory works;
the marker treats it like a stack (word-aligned, conservative, interior pointers accepted) [V].
Two things follow:

1. Registering and unregistering a range per call is far too slow (`GC_Roots_Add` mallocs a
   node, `GC_Roots_RemoveByRange` walks the list) [V]. The frame `Vec<Vec<u64>>` should become
   **one contiguous slot stack per (green) thread**, registered once for its whole capacity and
   re-registered when it grows; each call takes a window of it, as an interpreter stack would.
   This is also the natural shape for green threads (one slot stack each) and stack traces [I].
2. Everything else that can hold the only reference to an object must either be allocated
   through `scalanative_GC_alloc` and be reachable from a root, or be registered: the module
   table, string literals and interned strings, static fields, the callback table, `Zone`/
   `stackalloc` memory and pending exception values. Memory the VM allocates with `heap.rs`
   (RTTI objects, class-name strings) is outside the Immix heap, so the marker ignores it; the
   objects it points to (e.g. `Class.name` strings if allocated in Immix) would be lost unless
   the RTTI area is registered as a root range or RTTI is allocated through Immix and kept
   reachable from a registered table [I].

The interpreter's own Rust stack is scanned conservatively between `scalanative_GC_init`'s frame
and the collection point, which covers `u64` object addresses in Rust locals and registers
(`RegistersCapture`) without any rooting discipline [V].

### 3.4 Integration plan and effort

1. Copy `gc/immix`, `gc/immix_commix`, `gc/shared` (and `StackTrace.c` or a stub) into
   `native/`, compile with `-DSCALANATIVE_GC_IMMIX` in `crates/snrt/build.rs`; add the C file
   with the `__*` symbols and setters. — 1 day.
2. Write `refFieldOffsets` (from `Field.ty`) into every RTTI; reserve a contiguous id range for
   weak references; set the `__*` ids after bootstrap. — 1–1.5 days.
3. Replace `natives/gc.rs`: `scalanative_GC_alloc*` calls the C functions directly (a plain
   `extern "C"` call, no libffi) and the VM's own allocations of objects go the same way. —
   0.5 day.
4. One contiguous slot stack per thread, registered with `scalanative_GC_add_roots`; register
   the module table, literal/intern tables, statics and the callback table; call
   `scalanative_GC_init` at the top of `run_main`. — 2–3 days.
5. `collect`, heap-size queries, stats and the weak-reference callback wired to the C
   implementations; a stress program that allocates far more than it keeps; run the Scala
   Native suite under `GC_MAXIMUM_HEAP_SIZE` pressure and fix missed roots. — 2–3 days.

**Total 6–10 days.** Steps 2 and 4 are needed by any collector (including our own), so the
collector itself costs about one day. Debug aids exist: `GC_ASSERTIONS`, `GC_STATS_FILE`,
`SCALANATIVE_GC_LOG_LEVEL` [V].

### 3.5 Threads, later

- **Green threads (ADR 0003):** stay in the single-threaded build. Each green thread owns a
  registered slot stack; if green threads get their own native stacks (stackful switching),
  register those ranges too, since only the current OS stack is scanned automatically [I].
- **OS threads:** rebuild with `-DSCALANATIVE_MULTITHREADING_ENABLED`, create threads through
  `scalanative_GC_pthread_create`, poll `scalanative_GC_yield` at the interpreter's back-edges
  and calls (the same places green threads switch), bracket blocking C calls with
  `scalanative_GC_set_mutator_thread_state`, and adopt the 16-byte header with the lock word
  (`layout.rs` constants and `ObjectHeader.h` agree once `USES_LOCKWORD` is on) [V for the C
  side, I for the VM side]. Commix is a drop-in for parallel marking then [V].

### 3.6 Risks

- Coupling to Scala Native's GC sources: they change with 0.6 (the ADR already plans a port),
  but the vendored copy is 4.3k lines of plain C we can keep as-is; risk low–medium [I].
- Conservative roots keep some garbage alive (a slot holding a stale address); same as ADR
  0004's own design [V].
- `Heap_Init` reserves the maximum heap (physical memory size by default) as virtual memory;
  fine on both targets, but set `GC_MAXIMUM_HEAP_SIZE` in tests [V].
- Immix is not generational; for short scripts that mostly allocate garbage that is acceptable
  and Scala Native's own numbers back it [V].

### 3.7 The other three

- **`none`** is a bump allocator over `mmap`ed chunks: what we have today, in C [V].
- **`boehm/gc.c`** is the shim discussed in §2 [V].
- **Commix** is Immix with parallel marking and concurrent sweeping on GC threads; it needs the
  multithreaded build and brings nothing until OS threads exist [V].

## 4. MMTk (mmtk-core)

- **What.** Rust memory-management toolkit, `mmtk` 0.33.0 (2026-08-06), MIT OR Apache-2.0,
  MSRV 1.84; Tier 1 `x86_64-unknown-linux-gnu`, Tier 2 `aarch64-apple-darwin` [V]. Plans
  include NoGC, MarkSweep (free-list allocator modelled on mimalloc, never moves), Immix
  (opportunistic evacuation; the `immix_non_moving` feature "disables any object copying in
  Immix", and since 0.32 each Immix space can be configured non-moving), StickyImmix, SemiSpace,
  generational plans [V]. Bindings: OpenJDK, JikesRVM, V8, Julia, Ruby (mmtk-ruby: NoGC,
  MarkSweep, Immix, StickyImmix; Ruby 3.4's modular GC hook) [V].
- **Conservative roots and pinning.** `vo_bit` feature: "global valid object (VO) bit metadata…
  set when an object is allocated"; `memory_manager::is_mmtk_object(addr)` and
  `find_object_from_internal_pointer` ("recommended for conservative stack scanning when internal
  pointers may exist") identify objects from arbitrary words; `object_pinning` feature with
  `pin_object`/`unpin_object` keeps them in place in moving plans [V]. The old `is_mmtk_object`
  Cargo feature was removed in 0.33.0 (now under `vo_bit`) [V]. With MarkSweep or non-moving
  Immix nothing moves anyway [V].
- **Binding effort.** A binding implements `VMBinding` with `ActivePlan`, `Collection`,
  `ObjectModel`, `ReferenceGlue`, `Scanning`, `Slot` and `MemorySlice`; `ObjectModel` declares
  where mark, forwarding, pinning, LOS and log bits live (in the header or in side metadata),
  `OBJECT_REF_OFFSET_LOWER_BOUND`, `get_current_size`, `ref_to_object_start`, etc.; object
  references must be word-aligned; `Collection` must implement `stop_all_mutators`,
  `resume_mutators`, `block_for_gc` and `spawn_gc_thread` — MMTk runs collection on **GC worker
  threads the binding spawns**, even for a single-threaded mutator [V]. The porting guide:
  "the refactoring within V8 required to get a simple NoGC plan working was substantial,
  touching over 100 files" and "a good idea not to underestimate the difficulty of a NoGC
  port" [V]. Our layout fits (side metadata means no header bits needed; `refFieldOffsets`
  drives `Scanning::scan_object`), but the binding, the worker threads, the side-metadata
  conventions and the API churn between releases put this at **3–6 weeks** [I].
- **Verdict.** The right choice if we later want generational or concurrent collection without
  writing it, and it is Rust. Too much for "a minimal GC soon". Maintenance risk medium (active,
  API changes each minor release) [V].

## 5. Rust GC crates

- **gc-arena** 0.7.0 (2026-08-08, MIT/CC0): safe incremental mark-sweep inside an arena that is
  only reachable through `mutate` callbacks; single-threaded; collection happens between
  `mutate` calls, so the interpreter would have to be stackless (as `piccolo` is). It manages
  Rust values, not raw memory in Scala Native's layout. Unsuitable [V].
- **dumpster** 2.1.0 (2026-02-04, MPL-2.0): reference counting plus cycle detection with a
  `Trace` derive; Rust-typed `Gc<T>` handles, per-object counts. Unsuitable [V].
- **rust-gc** (`gc` 0.5.1, 2025-07-14, MPL-2.0): simple thread-confined mark-sweep over
  `Gc<T>` with `Trace`/`Finalize`; not for a fixed foreign layout or millions of small objects.
  Unsuitable [V].

## 6. Comparison

| | Scala Native Immix (C, vendored) | Boehm via SN shim | MMTk | cppgc / Oilpan | Own mark-sweep (ADR 0004 as written) |
|---|---|---|---|---|---|
| Non-moving | yes [V] | yes [V] | MarkSweep yes; Immix with `immix_non_moving` [V] | compaction per space, off by default [V] | yes |
| Our layout, RTTI-driven heap tracing | native to it [V] | conservative, or precise via typed descriptors [V/I] | via `Scanning` impl [V] | no: needs `GarbageCollected<T>`/`Member<T>` [V] | yes |
| Conservative roots from Rust-heap slots | `scalanative_GC_add_roots` + interior pointers [V] | `GC_add_roots` [V] | `vo_bit` + `find_object_from_internal_pointer` [V] | stack only [V] | own bytemap |
| Weak references | built in [V] | disappearing links in shim [V] | `ReferenceGlue` (work) [V] | `WeakMember` [V] | to write |
| Embeddable from Rust, macOS arm64 + Linux x86-64 | same `cc` build as `libsnrt` [V] | vendored CMake/autotools or system lib [V] | crate, Tier 2 macOS arm64 [V] | Bazel/V8 build; standalone repo archived [V] | n/a |
| Licence | Apache-2.0 [V] | MIT-style [V] | MIT/Apache-2.0 [V] | BSD-3 [V] | ours |
| Effort to first working GC | **6–10 days** [I] | 4–7 days [I] | 3–6 weeks [I] | not viable | 8–15 days [I] |
| Performance | bump allocation, precise heap; SN: ~10% GC overhead vs Boehm's ~100% [V] | free-list, fully conservative [V] | production-grade [V] | — | unknown, ours to tune |
| Green threads | register each thread's slot stack [I] | same [I] | mutators are per-thread bindings [V] | thread-local heaps [V] | same |
| OS threads | `MULTITHREADING_ENABLED` build, safepoints, lock-word header [V] | `GC_THREADS`, register threads [V] | designed for it; binding does stop-the-world [V] | — | to write |
| Maintenance risk | low–medium (vendored, changes with SN 0.6) [I] | low [V] | medium (API churn) [V] | high (archived export) [V] | ours forever |

## 7. Recommendation

**Adopt Scala Native's Immix, compiled into `libsnrt`.** It is the only ready-made collector
that already knows our object layout and RTTI, traces the heap precisely, scans roots
conservatively (including any range we hand it), never moves objects, and was tuned on exactly
the workloads our libraries produce. Its cost is dominated by VM work we owe to any collector
(reference offsets in RTTI, a contiguous root-registered slot stack, root registration of VM
tables). It also fixes the two open items ADR 0004 flags — weak references and heap-size
reporting — and has a documented path to OS threads.

**Runner-up: Boehm through `gc/boehm/gc.c`** with a vendored or system `libgc`. Choose it if
the Immix integration hits a blocker (e.g. class-id ranges or the RTTI area as roots) — it needs
none of the `__*` symbols and no `refFieldOffsets`. Expect roughly 10× the GC overhead and a
build dependency.

**Keep MMTk on the phase-2 list** as the route to generational/concurrent collection if
measurements ever call for it. Drop cppgc and the Rust GC crates from consideration.

### Changes to ADR 0004

- Decision: replace "a mark-sweep collector of our own … size-class free lists … side table of
  object starts" with "Scala Native's Immix (v0.5.12 sources, Apache-2.0) compiled into
  `libsnrt` with `-DSCALANATIVE_GC_IMMIX`, driven through `scalanative_GC_*`". The heap
  structure (bytemap, blocks, lines, large-object space) comes with it.
- Keep: non-moving; precise heap through `refFieldOffsets`; conservative roots from frame
  slots; module instances, literals, statics, the callback table, `Zone`/`stackalloc` memory as
  roots. Add: the interpreter's Rust stack is scanned as well, which removes any rooting
  discipline for Rust locals.
- Add prerequisites: `refFieldOffsets` and a contiguous weak-reference id range in RTTI;
  definitions for the `__*` symbols; one contiguous slot stack per thread registered with
  `scalanative_GC_add_roots`; `scalanative_GC_init` at the top of `run_main`;
  `StackTrace_PrintStackTrace` available.
- Consequences: add "when OS threads arrive, the header grows by a lock word and the VM polls
  `scalanative_GC_yield`"; note Boehm as the fallback instead of the rejected alternative; note
  the coupling to Scala Native's GC sources (vendored, re-vendored with the 0.6 port).

## Sources

Local (verified in the checkout):
- `ref/scala-native/nativelib/src/main/resources/scala-native/gc/{immix,immix_commix,shared,boehm,none}/` — `ImmixGC.c`, `Marker.c`, `Heap.c`, `Object.c`, `Allocator.c`, `MutatorThread.{h,c}`, `WeakReferences.c`, `GCRoots.{h,c}`, `headers/ObjectHeader.h`, `shared/ScalaNativeGC.h`, `boehm/gc.c`, `none/gc.c`
- `ref/scala-native/tools/src/main/scala/scala/scalanative/codegen/{Generate,RuntimeTypeInformation}.scala`, `build/{GC,NativeLib,LLVM}.scala`; `nativelib/src/main/scala/scala/scalanative/runtime/Class.scala`
- `crates/vm/src/{heap.rs,rtti.rs,layout.rs,natives/gc.rs,interp/{mod,exec,invoke}.rs,class/fields.rs}`, `crates/snrt/build.rs`, `native/LICENSE-scala-native.md`
- `docs/research/03-interpreter-architecture-rust.md` §2.3 (earlier GC survey)

Web (fetched 2026-09-25):
- V8, "Oilpan library" blog post: https://v8.dev/blog/oilpan-library
- V8 `include/cppgc/README.md`: https://github.com/v8/v8/tree/master/include/cppgc
- Standalone export (archived 2025-11-14) and its BSD-3 licence: https://github.com/oilpan-gc/cppgc
- Node.js 20.6.0 release notes (cppgc for addons): https://nodejs.org/en/blog/release/v20.6.0 ; C++ addons docs: https://nodejs.org/api/addons.html
- Node Congress 2026 talk (cppgc in Node.js, Deno, Workers): https://gitnation.com/contents/garbage-collection-between-v8-cppgc-oilpan-and-native-runtimes
- `v8::cppgc` Rust module (v8 152.2.0): https://docs.rs/v8/latest/v8/cppgc/index.html ; Deno on rusty_v8: https://deno.com/blog/rusty-v8-stabilized
- bdwgc repository and README: https://github.com/bdwgc/bdwgc ; releases: https://github.com/bdwgc/bdwgc/releases ; `gc.h`: https://github.com/bdwgc/bdwgc/blob/master/include/gc/gc.h ; `gc_typed.h`: https://github.com/bdwgc/bdwgc/blob/master/include/gc/gc_typed.h ; interface doc: https://github.com/bdwgc/bdwgc/blob/master/docs/gcinterface.md ; Sep 2026 8-0 branch PR: https://github.com/bdwgc/bdwgc/pull/963
- Homebrew `bdw-gc` (8.2.12, arm64 bottles): https://formulae.brew.sh/formula/bdw-gc
- `bdwgc-alloc` crate: https://docs.rs/crate/bdwgc-alloc/latest ; repo: https://github.com/raviqqe/bdwgc-alloc ; `boehm_gc` prototype: https://docs.rs/boehm_gc
- Scala Native runtime/GC settings: https://scala-native.org/en/latest/user/runtime.html ; 0.3.8 release notes (Immix vs Boehm numbers): https://github.com/scala-native/scala-native/releases/tag/v0.3.8 ; VirtusLab, "Revisiting Scala Native performance": https://virtuslab.com/blog/scala/scala-native-performance
- MMTk core: https://github.com/mmtk/mmtk-core ; releases: https://github.com/mmtk/mmtk-core/releases ; `Cargo.toml` features: https://github.com/mmtk/mmtk-core/blob/master/Cargo.toml ; `memory_manager` API: https://docs.mmtk.io/api/mmtk/memory_manager/index.html ; `ObjectModel`: https://docs.mmtk.io/api/mmtk/vm/trait.ObjectModel.html ; `Collection`: https://docs.mmtk.io/api/mmtk/vm/trait.Collection.html ; porting guide: https://docs.mmtk.io/portingguide/howto/nogc.html and https://docs.mmtk.io/portingguide/howto/next_steps.html
- MMTk Ruby: https://github.com/mmtk/mmtk-ruby ; Rails at Scale on Ruby 3.4 modular GC: https://railsatscale.com/2025-01-08-new-for-ruby-3-4-modular-garbage-collectors-and-mmtk/ ; practitioner report: https://railsatscale.com/2025-09-16-reworking-memory-management-in-cruby/
- gc-arena: https://lib.rs/crates/gc-arena ; dumpster: https://lib.rs/crates/dumpster ; rust-gc: https://lib.rs/crates/gc
