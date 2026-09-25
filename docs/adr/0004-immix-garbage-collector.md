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
- runtime type information with filled `refFieldOffsets`, and class ids numbered so every
  class's subclasses form one contiguous range;
- the symbols Scala Native's linker normally generates (`__modules`, `__object_array_id` and
  the other array ids, the weak-reference ids);
- interpreter frames in one contiguous slot stack, registered with `scalanative_GC_add_roots`,
  plus the VM's own references (module instances, literals, statics, callback table);
- a stub for the stack-trace printing Immix calls.

Boehm through Scala Native's `gc/boehm` shim stays the fallback; MMTk is a later option if a
collector family beyond Immix is needed.

## Consequences
- Memory is reclaimed without moving objects, with the collector Scala Native itself defaults
  to, so the runtime behaves as in a Scala Native binary.
- Class ids stop being assigned in load order: loading a class must keep hierarchy ranges
  contiguous (for example by reserving id ranges per package or renumbering at collection).
- The frame representation changes from pooled `Vec`s to a slot stack; green threads
  (ADR 0003) give each thread its own registered stack.
- The VM depends on more of Scala Native's C code and its internal interfaces, which the move to
  Scala Native 0.6 must port along.
