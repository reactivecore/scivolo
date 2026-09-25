# ADR 0004 — A minimal non-moving mark-sweep collector

Status: proposed · Date: 2026-09-25

## Context
The VM's heap never frees memory (ADR 0002). Stress tests and long-running scripts allocate
without bound; boxing in generic collections alone allocates per element. Objects keep Scala
Native's layout and their addresses are handed to C, so objects must not move. Scala Native
itself defaults to Immix, a non-moving mark-region collector, and supports Boehm.

## Decision
A mark-sweep collector of our own, non-moving, run at allocation when the heap has grown by a
threshold since the last collection:
- **Heap:** size-class free lists for small objects plus separately allocated large objects;
  a side table of object starts, so any word can be tested for "points to an object".
- **Tracing:** precise inside the heap. Every class knows its reference fields from its layout,
  and object arrays hold references only.
- **Roots, conservative:** the interpreter's frame slots are untyped words, so every slot value
  that is an object start is a root. Also module instances, string literals, static variables,
  the current thread objects and values held by native code (the callback table, `stackalloc`
  and `Zone` memory) count as roots.
- **Reports:** `scalanative_GC_collect`, the heap-size queries and weak references talk to this
  collector.

Boehm was the alternative: it is ready-made, but it needs a system library or a vendored build,
and it scans everything conservatively.

## Consequences
- Memory is reclaimed without moving objects, so addresses given to C stay valid.
- Conservative roots may keep some garbage alive; the heap interior stays precise.
- Green threads (ADR 0003) add each thread's frames to the roots; OS threads would add
  safepoints later.
- The bump allocator is replaced, so allocation gets a little slower in exchange for bounded
  memory.
