# ADR 0003 — Green threads first, OS threads later

Status: accepted · Date: 2026-09-25

## Context
The VM runs one thread and reports multithreading disabled, so Scala Native's concurrent tests
skip, blocking sockets hang and waits with timeouts fail. Java threads can be mapped onto
OS threads, which requires a heap, caches and a callback table safe for concurrent use, or onto
green threads the VM schedules itself on one OS thread.

## Decision
Implement `java.lang.Thread` first as green threads: the VM switches between interpreted
threads at calls, loop back-edges and blocking operations, on one OS thread. They also serve
as the implementation of Java virtual threads. Real OS threads stay on the roadmap as a later
step.

## Implementation choices (2026-09-27)
From `docs/research/05-threads.md`:
- Each green thread is a stackful coroutine (`corosensei`) with its own native stack and frame
  stack; the scheduler switches at parks, sleeps, monitor waits, `Thread.yield` and thread exit,
  and preempts by a tick counter at loop back-edges and calls (the runtime spin-waits).
- Externs marked `blocking` run on helper OS threads while the scheduler continues, their `errno`
  copied back; a program with one thread calls them inline. Sleeps, timed parks and waits are
  scheduler timers.
- The C runtime stays the single-threaded Immix build; its root scan covers every green thread's
  native and frame stacks (an addition to ADR 0004's patch).
- No lock word in the object header: monitors, `wait`/`notify` and `holdsLock` are a VM side
  table keyed by object address, and `MemoryLayout`'s offset accessors are overridden.
- Java virtual threads are reported unsupported (`isContinuationsSupported = false`) until they
  are built on green threads; Scala Native's `delimcc` continuations are not used.

## Consequences
- The VM stays single-threaded inside: no locks in the interpreter, heap or caches.
- A C call that blocks but is not marked `blocking` stops every green thread.
- Programs that need parallel speed-up do not get it until OS threads land.
- Monitors, `LockSupport.park` and the `Thread` runtime hooks have to be implemented against the
  scheduler, with link-time multithreading reported as enabled.
