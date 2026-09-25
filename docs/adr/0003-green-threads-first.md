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

## Consequences
- The VM stays single-threaded inside: no locks in the interpreter, heap or caches.
- A C call that blocks stops every green thread; blocking I/O the javalib does through C
  (sockets, `waitpid`, `sleep`) must be turned into non-blocking waits the scheduler drives.
- Programs that need parallel speed-up do not get it until OS threads land.
- Monitors, `LockSupport.park` and the `Thread` runtime hooks have to be implemented against the
  scheduler, with link-time multithreading reported as enabled.
