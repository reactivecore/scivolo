# ADR 0004 — Scala Native's Immix collector for the VM heap

Status: accepted · Date: 2026-09-25

## Context
The VM's heap never frees memory (ADR 0002), so stress tests and long-running scripts grow
without bound. Objects keep Scala Native's layout and their addresses are handed to C, so they
must not move. The options compared in `docs/research/04-gc-options.md` were Scala Native's own
collectors, Boehm, MMTk, V8's cppgc and Rust GC crates.

## Decision
Build Scala Native's Immix collector (C sources from the nativelib jar, v0.5.12) into the C
runtime library and allocate through its `scalanative_GC_*` interface. Immix traces the heap
precisely through each class's `refFieldOffsets`, finds roots conservatively, never moves
objects, and supports weak references and a multithreaded mode.

What the VM provides:
- runtime type information with filled `refFieldOffsets` and the class ids Immix checks;
- the symbols Scala Native's linker normally generates (`__modules`, `__object_array_id` and
  the other array ids, the weak-reference ids);
- interpreter frames in one contiguous frame stack the collector scans, plus roots for the VM's
  own references (module instances, literals, statics);
- a stub for the stack-trace printing Immix calls.

Boehm through Scala Native's `gc/boehm` shim stays the fallback; MMTk is a later option if a
collector family beyond Immix is needed.

## Implementation (2026-09-25)
Where the implementation differs from the plan above:
- `gc/immix`, `gc/immix_commix` and `gc/shared` are vendored under `native/nativelib/scala-native/gc/`
  and compiled with `-DSCALANATIVE_GC_IMMIX`. `native/scivolo/gc_glue.c` defines the linker
  symbols with setters, `currentThreadInfo`, `scivolo_GC_init(low, high)` and the stack-trace stub.
- Patches to Immix: `Marker_MarkRoots` also scans a table of green-thread roots
  (`scivolo_threads`): every frame stack from its low end to the current top, since registering
  the whole reserved region would scan stale frames, and the native stack of each suspended
  thread; `scivolo_GC_switch_stack` points the collector at the running thread's native stack.
  `ObjectHeader.h` declares the class id symbols non-`const`.
- The weak-reference callback only raises a flag in C; the VM runs the program's handler at the
  next interpreted call, never inside the allocation that collected.
- The last 16 MiB of the frame stack are kept for constructing `StackOverflowError`.
- Immix's state is process-global, so one process runs one VM (`Heap::new` refuses a second).
- Hierarchy-ordered class ids turned out unnecessary: the VM answers type tests itself, and Immix
  only needs the array ids and one range for weak references. `java.lang.ref.WeakReference` and
  its subclasses take ids from 2^24 upwards (`gcinfo.rs`).
- Module instances, string literals, link-time strings, class metadata, global variables and
  constants live in a never-freed permanent heap whose chunks are registered as conservative
  roots, so `__modules` stays empty and the VM's hash maps may hold those addresses.
  2026-10-22: the permanent heap fills a fixed-address region first (ADR 0034).
- Frames, `stackalloc` memory and escaping `var`s share one mmap'd frame stack (`stack.rs`),
  freed when a call returns. Aggregate values built by `insert` are blob arrays in the collected
  heap, kept alive by the interior pointers in frames.
- The native stack of the interpreter thread is scanned from the collection point to the
  stack's high end, so Rust locals holding objects between allocations stay live.

## Consequences
- Memory is reclaimed without moving objects, with the collector Scala Native itself defaults
  to, so the runtime behaves as in a Scala Native binary.
- Object addresses the VM keeps must be in the permanent heap, the frame stack or on the native
  stack; an address kept only in Rust heap memory (a `Vec`, a map, a `Vm` field) is invisible to
  the collector.
- Aggregate (struct) values are addresses of their bytes: `store` copies the bytes and `load`
  yields the address, so no pointer to a collected blob ends up in memory.
- The frame representation changes from pooled `Vec`s to a frame stack; green threads
  (ADR 0003) give each thread its own frame stack, and the marker must scan all of them.
- The VM depends on more of Scala Native's C code and its internal interfaces, which the move to
  Scala Native 0.6 must port along.
