# 05 — Threads: how Scala Native 0.5.12 does it, and how the VM should

Date: 2026-09-27. Claims are [V] verified by reading the code, or [I] inferred. Paths are
relative to `ref/scala-native` (v0.5.12) unless they start with `crates/`, `native/` or `tmp/`.

**Recommendation in one paragraph.** Keep ADR 0003 (green threads on one OS thread) and
implement it with stackful coroutines (`corosensei`): every Java thread is a coroutine with its
own native stack and its own frame stack, switched by a scheduler at parks, sleeps, monitor
waits, `Thread.yield`, and by a tick counter at loop back-edges and calls. Blocking C externs
(the 106 `@blocking` ones) run on helper OS threads while the scheduler keeps other green
threads running; a program with a single thread keeps calling them inline. Immix stays the
single-threaded build; only its root scan is taught about the coroutine stacks. The object
header keeps no lock word: the VM implements monitors itself by overriding the six runtime
methods `synchronized`/`wait`/`notify` lower to. `isMultithreadingEnabled` becomes `true`,
`isContinuationsSupported` `false`. ADR 0003 needs three amendments (section 4.3).

## 1. How Scala Native 0.5.12 implements threads

### 1.1 The link-time switch and the object header

- `LinktimeInfo.isMultithreadingEnabled` is a `@resolvedAtLinktime` property
  (`nativelib/.../meta/LinktimeInfo.scala:59-62`) [V]. The NIR jars contain `linktime if`
  instructions on it; the VM evaluates them at run time (`crates/vm/src/interp/exec.rs:70-79`)
  and answers `false` today (`crates/vm/src/linktime.rs:28-31`) [V]. Flipping it is one line.
- `isVirtualThreadsSupported = isMultithreadingEnabled && isContinuationsSupported`, and
  `isContinuationsSupported` is computed from `target.os`/`target.arch` (Linux or macOS, not
  arm32, not 32-bit) — so on both our targets it is **true** as soon as multithreading is
  enabled (`LinktimeInfo.scala:50-57`) [V]. Virtual threads use `delimcc.c` continuations that
  copy native stack segments (`nativelib/.../delimcc.c`, `delimcc.h`) [V]; copying the
  interpreter's Rust frames is not viable, so the VM must override `isContinuationsSupported`
  to `false` (it is a computed accessor, which `linktime.rs` already calls as a method) [V/I].
- **Header layout changes with multithreading.** `MemoryLayout.scala:24-30,55-73`: with
  multithreading the object header is `{rtti, lockWord}` (fields start at 16), arrays are
  `{rtti, lockWord, length, stride}` (length at 16), and the RTTI header also gains a lock
  word (`tools/.../codegen/CommonMemoryLayouts.scala:14-38`, `Metadata.scala:17`) [V]. The C
  runtime mirrors it: `ObjectHeader.h:25-29` defines `USES_LOCKWORD` under
  `SCALANATIVE_MULTITHREADING_ENABLED`, and `build/LLVM.scala:90-93` passes that define only
  for multithreaded links [V]. The VM's constants are the single-threaded ones
  (`crates/vm/src/layout.rs:6-9`: header 8, array header 16, length at 8, stride at 12) and
  `native/` is compiled with `-DSCALANATIVE_GC_IMMIX` only (`crates/snrt/build.rs`) [V].
- So the layout is a **link-time choice the VM makes**, but not a free one: the NIR in the
  jars asks `MemoryLayout$Object$.FieldsOffset`, `MemoryLayout$Array$.LengthOffset` etc. as
  ordinary method calls (they are `@alwaysinline def`s on the linktime property; the inlining
  happens in Interflow at link time, not in the jars) [V/I]. Whatever the VM decides, it must
  override those accessors to return its own constants; otherwise `Arrays.scala` and friends
  compute offsets that disagree with `layout.rs`.

### 1.2 `synchronized`, monitors, `wait`/`notify`

- nscplugin lowers `synchronized` to `scala.scalanative.runtime.package$.enterMonitor(obj)`,
  the body under a try/finally, and `exitMonitor(obj)` in the finally
  (`nscplugin/src/main/scala-3/.../NirGenExpr.scala:1987-2040`) [V]. This is in the jars; the
  VM runs it today, and both are no-ops because of `if (isMultithreadingEnabled)`
  (`nativelib/.../runtime/package.scala:24-36`) [V].
- `getMonitor(obj)` wraps the lock word at `obj + LockWordOffset` in a `BasicMonitor`
  (`package.scala:40-52`) [V]. `BasicMonitor` (`runtime/monitor/BasicMonitor.scala`) is a thin
  lock: CAS of the thread's object address into the lock word through
  `runtime.ffi.stdatomic.atomic_compare_exchange_intptr` (C: `clib/.../stdatomic.c:266`),
  recursion count in 7 bits, and inflation into an `ObjectMonitor` object whose address is
  stored with the low bit set (`monitor/package.scala:9-36`) [V]. Contention spins, then
  `NativeThread.currentNativeThread.sleepNanos(backoff)` (`BasicMonitor.scala:168-204`) [V].
- `ObjectMonitor` (`monitor/ObjectMonitor.scala`, 707 lines) is pure Scala: entry list, wait
  set, CAS on its own fields, and it blocks threads with `LockSupport.park`/`parkNanos` and
  wakes them with `LockSupport.unpark` (lines 148-155, 252-272, 359-360, 507) [V].
- `Object.wait/notify/notifyAll` are `_Object.__wait/__notify/__notifyAll` in
  `nativelib/.../runtime/Object.scala:27-45`, each `getMonitor(this)._wait(...)` under the
  same `if (isMultithreadingEnabled)` [V]. `Thread.holdsLock` → `NativeThread.holdsLock` →
  `getMonitor(obj).isLockedBy` (`NativeThread.scala:163-165`) [V].
- Module initialisation under multithreading goes through `scalanative_initializeModule` /
  `scalanative_awaitForInitialization` (`package.scala:233-297`), generated by Lower, i.e.
  not in the jars: the VM keeps its own `Instr::Module` path [V].

### 1.3 `java.lang.Thread`, `NativeThread`, `PosixThread`

- `Thread.start()` → `startInternal()`: `if (!isMultithreadingEnabled) UnsupportedFeature.threads()`
  then `platformCtx.start(this)` (`javalib/.../Thread.scala:279-285`) [V]; the platform
  context creates `PosixThread` (`Thread.scala:576-578`) [V].
- `new PosixThread` (`javalib/.../impl/PosixThread.scala:45-92`): inits a pthread mutex and
  two condition variables inside a byte array field (`_state`), then calls
  `GC.pthread_create(id, attrs, NativeThread.threadRoutine, threadRoutineArgs(this))` [V].
  `GC.pthread_create` is the extern `scalanative_GC_pthread_create` (`runtime/GC.scala:70-76`),
  implemented in `ImmixGC.c:164-174`: it wraps the routine so the new OS thread runs
  `MutatorThread_init` (registers with the GC), the routine, `MutatorThread_delete` [V].
- `NativeThread.threadRoutine` is a `CFuncPtr1.fromScalaFunction` (a C callback into
  Scala) whose argument is the raw address of the `NativeThread` object
  (`NativeThread.scala:207-212`) [V]. `threadEntryPoint` (`:215-248`): `stackalloc`,
  `TLS.assignCurrentThread` (C extern), `TLS.setupCurrentThreadInfo` (C),
  `StackOverflowGuards.setup` (C), state = Running, `atomic_thread_fence`, **spin** `while
  (thread.getState() == NEW) onSpinWait()`, `thread.run()`, uncaught-exception handler, and in
  `finally` `thread.synchronized { onTermination(); state = Terminated; thread.notifyAll() }`
  [V]. `onTermination` removes the thread from `NativeThread.Registry` (a
  `ConcurrentHashMap`) and pokes `MainThreadShutdownContext` [V].
- `join` = `synchronized { while (isAlive()) wait() }`, timed variant with `wait(millis,
  nanos)` (`Thread.scala:291-330`) [V]. `isAlive` reads `NativeThread.state` [V].
- `Thread.sleep` → `PosixThread.sleep` → `sleepInterruptible`: creates a **pipe**, then
  `poll(pipefd, 1, millis)` in a loop; `interrupt()` writes a byte to the pipe
  (`PosixThread.scala:114-124, 205-237`) [V]. `sleepNanos` uses `nanosleep` (`:239-258`) [V].
- `LockSupport.park/parkNanos/parkUntil/unpark` → `platformCtx.nativeThread.park()` etc.
  (`javalib/.../locks/LockSupport.scala:20-67`) → `PosixThread.park(time, isAbsolute)`:
  permit counter via `AtomicInt` on the `counter` field (`classFieldRawPtr`), then
  `pthread_mutex_trylock` + `pthread_cond_wait`/`pthread_cond_timedwait`; `unpark` sets the
  counter and `pthread_cond_signal`s (`PosixThread.scala:126-199`) [V]. When multithreading
  is off, `park` instead drains `NativeExecutionContext.queueInternal` (`NativeThread.scala:49-66`)
  — this is how `Future`/`Await` work single-threaded today [V].
- `Thread.yield` → `PosixThread.yieldThread` → `sched_yield` (C) (`Thread.scala:675-681`,
  `PosixThread.scala:431`) [V]. `onSpinWait` → `llvm.donothing` (VM: no-op, `natives/llvm.rs:94`) [V].
- `Thread.currentThread()` → `NativeThread.currentThread` → C TLS
  `scalanative_currentThread` (`nativeThreadTLS.c:22-33`); the VM replaces the three TLS
  externs with one permanent cell (`crates/vm/src/natives/runtime.rs:7-31`) [V].
- Shutdown: after `main`, Scala Native calls `runtime.onShutdown()` (`package.scala:94-130`),
  which drains the queue and `LockSupport.park()`s until no non-daemon thread is alive [V].
  The VM's `run_main` does not call it today [V].

### 1.4 Volatile, atomics, thread locals, thread-local storage

- `@volatile` fields become NIR loads/stores with a memory order (`crates/nir/src/nir.rs:179-180`);
  `Op::Fence` exists too and the VM treats it as a no-op (`interp/ops.rs:31`) [V] — correct on
  one OS thread.
- `java.util.concurrent.atomic.*` and the runtime use `libc.stdatomic` /
  `runtime.ffi.stdatomic`: `@extern` wrappers over `scalanative_atomic_*` in
  `clib/.../stdatomic.c` (C, vendored in `native/clib/`), called on `classFieldRawPtr`
  addresses [V]. They already work through the VM's `dlsym` FFI (the atomic tests pass) [V].
- `ThreadLocal` is a Harmony port in Scala; values live in a per-`Thread` map;
  `isMultithreadingEnabled` only selects an atomic hash counter (`ThreadLocal.scala:435-440`) [V].
  The VM needs nothing beyond a correct `currentThread`.
- C thread-local storage (`SN_ThreadLocal`): `currentThread`, `currentNativeThread`,
  `currentThreadInfo` (`nativeThreadTLS.c`), `currentMutatorThread` (`gc/immix/State.h:15`),
  and `scalanative_GC_yieldpoint_trap`, the only NIR global emitted as `thread_local`
  (`codegen/llvm/AbstractCodeGen.scala:225-243`) [V]. No javalib symbol is `@threadLocal`.

### 1.5 Safepoints, yield points, blocking externs

- Lower inserts `scalanative_GC_yield()` calls before backward jumps and before returns, only
  when multithreading is on, the GC is Immix/Commix, and the method has more than 4
  instructions or a loop (`codegen/Lower.scala:258-270, 874-903`) [V]. With trap-based yield
  points (the default, `build/NativeLib.scala:141`) the LLVM backend emits a volatile load
  through the thread-local trap page instead (`AbstractCodeGen.scala:1040-1060`,
  `docs/contrib/gc-safepoint-trampoline.md`) [V]. **None of this is in the NIR jars**; the VM
  decides its own preemption points.
- `scalanative_GC_yield` (`ImmixGC.c:181-186`) checks `Synchronizer_stopThreads` and parks
  the thread in `Synchronizer_yield` (`Synchronizer.c:428-442`) [V]. The whole synchronizer is
  compiled only under `SCALANATIVE_MULTITHREADING_ENABLED` (`Synchronizer.c:1`) [V].
- `@blocking` externs: nscplugin records `Attr.Extern(blocking = true)`
  (`NirGenStat.scala:74,337`; decoded by `crates/nir/src/decode/defn.rs:65`) [V]. Lower wraps
  such calls in `scalanative_GC_set_mutator_thread_state(Unmanaged)` … call …
  `scalanative_GC_yield` … `(Managed)` (`Lower.scala:1016-1075`), and `Blocker.begin/end` for
  virtual-thread carriers [V]. 106 externs carry `@blocking` in javalib/posixlib/clib/nativelib
  (`read`, `poll`, `accept`, `recv`, `waitpid`, `nanosleep`, `usleep`, `lockf`, `pthread_cond_wait`,
  `sem_wait`, …) [V, grep count]. The jars only carry the attribute, so the VM sees exactly
  where a C call may block.

### 1.6 Immix in multithreaded mode

`-DSCALANATIVE_MULTITHREADING_ENABLED` turns on: a `MutatorThread` per OS thread with its own
allocator (`MutatorThread.c:22-77`), the mutator list, `Synchronizer_acquire` stop-the-world
before marking (`Heap.c:163-166`), per-mutator recycling of blocks (`Heap.c:233-247`), and root
scanning of every mutator's native stack from the `stackTop` saved at the last
`switchState(Unmanaged)` plus its captured registers (`Marker.c:155-232`) [V]. Threads blocked
in C are "Unmanaged" and are simply scanned where they stopped [V]. Our single-threaded build
has one implicit mutator whose `stackTop` is set inside `Heap_Collect` [V].

### 1.7 Everything on the path, and who provides it

| Item | Kind | Today in the VM | With green threads |
|---|---|---|---|
| `linktimeinfo.isMultithreadingEnabled` | link-time prop | `false` | `true` |
| `LinktimeInfo.isContinuationsSupported` | computed accessor (NIR) | evaluates to `true` | override → `false` |
| `MemoryLayout$Object$/Array$/Rtti$` offset accessors | NIR methods | run as NIR (give single-threaded values) | override with `layout.rs` constants |
| `runtime.package$.enterMonitor/exitMonitor/getMonitor` | NIR (`@alwaysinline`) | no-op branch | override: VM monitors |
| `_Object.__wait/__notify/__notifyAll`, `NativeThread.holdsLock` | NIR | no-op branch | override: VM monitors |
| `BasicMonitor`, `ObjectMonitor` | NIR | unused | unused (replaced) |
| `scalanative_GC_pthread_create` | C (`ImmixGC.c`) | resolvable, never called | override: spawn green thread, run the routine (a C-callback trampoline the VM already understands) |
| `PosixThread.park/unpark/sleep/sleepNanos/interrupt/setPriority/onTermination`, `PosixThread$.yieldThread/defaultOSStackSize` | NIR over posixlib C | disabled branches | override: scheduler operations |
| `pthread_mutex_init/cond_init/...` in the `PosixThread` constructor | C (`native/posixlib/pthread.c`) | not reached | reached once per thread unless the constructor's `if` is short-circuited; harmless (one OS thread) or override the whole class |
| `scalanative_currentThread/currentNativeThread/assignCurrentThread` | C TLS | one cell | per green thread |
| `scalanative_setupCurrentThreadInfo`, `StackOverflowGuards_*` | C | stubbed to 0 | stubbed; VM sets its own per-thread floor |
| `sched_yield`, `nanosleep`, `poll` on the sleep pipe | C | never reached | never reached (methods overridden) |
| `scalanative_atomic_*`, `llvm.donothing` | C / intrinsic | work | unchanged |
| `scalanative_GC_yield`, `GC_set_mutator_thread_state`, `GC_yieldpoint_trap` | C, inserted by Lower | not in jars | not needed (single-threaded Immix) |
| `Attr.Extern(blocking)` | NIR attribute | ignored | offload the call to a helper OS thread |
| `runtime.onShutdown()` | NIR | not called | call after `main` (waits for non-daemon threads) |

## 2. What the skipped and failing tests depend on

Source: `tmp/sn-report/*.txt` (run of 2026-09-25, per-method lines plus a summary line per
group) [V]. The summary lines count methods: 3,117 skipped in total, 2,827 of them in
`java.util.concurrent`; `docs/conformance.md` counts classes (115).

- **"Requires multithreaded runtime"** is `AssumesHelper.assumeMultithreadingIsEnabled`
  (`unit-tests/shared/.../utils/AssumesHelper.scala:25-29`) on `Platform.isMultithreadingEnabled`,
  i.e. the link-time property [V]. It skips 80 classes in `java.util.concurrent` (the whole
  JSR-166 port: executors, queues, locks, `CompletableFuture`, `ForkJoin*`, `Phaser`, …) and
  `java.util.PriorityQueueTest` [V]. Flipping the property runs them; they then need real
  concurrency: `CountDownLatch`, `Thread.sleep`, timed `poll`/`await`, `Thread.join`,
  interrupts, `LockSupport.park` [V, sampled].
- `StackOverflowTest.{catchStackOverflowErrorInThread, entersFinallyBlocksInThread,
  exitsMonitorsInThread}` (`assumeTrue(Platform.isMultithreadingEnabled)`, line 86): a
  `StackOverflowError` inside a spawned thread [V] — needs a per-thread stack floor.
- `ModuleInitializationTest.initializeFromMultipleThreads`,
  `GarbageCollectorTest."cleans stale mutator threads"`, `gc.IssueTests.issue4445`,
  `ProcessTest.concurrentPipe`, `ProcessTestOnJDK9` (assumption `got: <false>`): all thread
  assumptions [V]. The GC test inspects Immix's mutator list and may stay skipped or fail
  under green threads (there is one mutator) [I].
- **`ProcessTest` failures** (4 timeouts, 2 `NoClassDefFoundError: UnsupportedFeature`): the
  helper wraps `scala.sys.process` in `Await.result(Future(pb.!(logger)), 30.seconds)`
  (`ProcessTest.scala:1127-1156`) [V]. `sys.process` spawns threads to pump stdout/stderr, and
  `Thread.start` throws `UnsupportedFeature.threads()` [V/I]; the `Await` then runs the queue
  until the 30/60 s timeout, which is why the group needs `SCALAI_TIMEOUT=300`. `waitFor` with
  a timeout is a `waitpid`+`Thread.sleep(100)` loop (`process/UnixProcess.scala:68-85`) and
  the process watcher is a thread only when multithreading is enabled
  (`GenericProcessWatcher.scala:14, 89-101`) [V].
- **`java.net` hang**: `ServerSocketTest.closeWithActiveAccept` runs `server.accept()` in a
  `Future`, sleeps one second, closes the socket and awaits (`ServerSocketTest.scala:84-99`)
  [V]. Single-threaded, the `Await` executes the future on the main thread and `accept` (a
  `@blocking` extern) blocks in C forever; the run hit the 300 s timeout after
  `ServerSocketTest.close` and `SocketTest` never ran (72 passed in an earlier partial run) [V].
  This one needs a blocking C call to run off the interpreter thread, not merely threads.
- Everything else in the failure list (stack traces, `Class.forName`, `getenv`, number
  parsing, `Nothing`-lambda miscompiles) is unrelated to threads [V].

## 3. Design options for the VM

Common ground for all options: the interpreter is recursive on the native stack — `run`
executes one method, `compute` → `call_direct` → `push_frame` → `run` for each call
(`crates/vm/src/interp/exec.rs:10-22, 49-117`, `invoke.rs:15-39`); slots live in one mmap'd
frame stack (`stack.rs:36-110`) whose `[low, top]` the patched Immix marker scans
(`native/nativelib/scala-native/gc/immix/Marker.c:225-232`, ADR 0004); the native stack is
scanned from the collection point to the thread's high end; `StackOverflowError` comes from a
native-stack floor (`interp/mod.rs:52-62, 157-162`); `Vm` is a single owner passed as
`&mut self`; C callbacks re-enter through a `thread_local!` slot table holding a `*mut Vm`
(`natives/callback.rs:21-64`, ADR 0002) [V].

### 3a. Green threads on one OS thread, stackful coroutines

- **Mechanics.** Each Java thread = `corosensei::Coroutine` on its own stack (the crate's
  `DefaultStack` is mmap'd with a guard page; Linux/macOS on x86_64 and aarch64 are supported;
  panics unwind through the coroutine into the resumer) [V, docs.rs 0.3.4] plus its own
  `Frames` (reserve less than the main thread's 1 GiB, e.g. 64 MiB, still `MAP_NORESERVE`).
  The scheduler lives on the OS thread's original stack; `run_main` becomes "resume the main
  green thread until it yields, then pick the next runnable one". Switch points: (1) the
  overridden runtime methods (park, sleep, monitor enter/wait, join via wait, yield,
  thread exit); (2) a preemption tick decremented at loop back-edges (`Instr::Jump` to a
  lower pc, which `code.rs` can mark at lowering time) and in `Vm::call`, yielding every N
  ticks. (2) is mandatory: `threadEntryPoint` spins on `getState() == NEW`, and
  `ObjectMonitor`/JSR-166 code spin with `onSpinWait` [V].
- **Blocking C calls.** Calls to externs with `Attr.Extern { blocking: true }` are handed to
  a helper OS thread (a small growable pool, Go's syscall hand-off in miniature); the green
  thread parks until the helper reports the result and the helper's `errno`, which the VM
  writes back into the interpreter thread's `errno` before resuming (javalib reads `errno`
  right after the call) [V that it reads errno; I for the mechanism]. When only one green
  thread exists, call inline as today, so single-threaded scripts lose nothing. Helper threads
  never touch `Vm`; the arguments are plain integers and addresses into a non-moving heap
  pinned by the caller's frame, so a concurrent collection on the VM thread cannot free them
  [I]. A C callback from a helper thread aborts, as ADR 0002 already specifies. `sleep`,
  `park` timeouts, `Object.wait(timeout)` and `join(millis)` do not use C at all: they are
  scheduler timers. `waitpid`, `accept`, `read`, `poll` go through the helper path; a later
  refinement can route fd waits through one `poll`/`kqueue` loop instead.
- **GC roots.** Immix must scan, for every green thread, its native stack from the saved
  stack pointer to the stack's high end (a suspended coroutine's callee-saved registers are
  on its stack at the switch point [I, standard for context switches]) and its frame stack
  `[low, top]`. `gc_glue.c` grows a small table of such ranges the VM keeps current; the
  `Marker_MarkRoots` patch iterates it instead of the single `scalai_frame_stack`. For the
  running coroutine, `currentThreadInfo.stackTop/stackBottom` and the mutator's `stackBottom`
  must be set to the coroutine's bounds at each switch (`MutatorThread.c:128-135` and the
  `SCALANATIVE_THREAD_ALT_STACK` branch of `Marker_markProgramStack` both read them) [V];
  otherwise a collection inside a coroutine scans from its sp up to the main stack's end
  across unmapped memory. The scheduler's own stack is registered as a custom root range.
  Immix itself stays single-threaded: one mutator, one allocator, no synchronizer.
- **`&mut Vm`.** A switch happens while `&mut Vm` borrows are live in the suspended
  coroutine's Rust frames; the running coroutine gets its own `&mut *vm` from a raw pointer,
  exactly the argument `callback.rs:51-55` already makes for re-entrant C callbacks: the
  suspended borrow is not used until resumed. Per-thread state moves out of `Vm` into a
  `GreenThread` (frames, the current-thread cell, the stack floor and `overflowing`, the trace
  stack); an audit must find any `&mut self.field` held across a call that can switch
  (`scratch`, the weak-reference flag) [I]. Rust panics stay confined to the coroutine.
- **Callbacks.** Unchanged: one OS thread, one `RUNNING` VM. A callback that parks its green
  thread suspends a C frame on that coroutine's stack; fine unless the C code holds
  OS-thread-affine state across the call (same caveat as Loom's pinning).
- **Portability.** `corosensei` covers both targets with inline asm and mmap'd stacks
  [V]; `makecontext`/`swapcontext` would be the fallback (deprecated on macOS, slow because of
  the signal-mask syscall) [I]. The `errno` copy-back uses `__error`/`__errno_location`,
  which `ffi/mod.rs:109-129` already wraps [V].
- **Trade-offs.** Robustness: high (no data races, deterministic switch points, the C
  runtime unchanged); effort: ~3–4 weeks [I]; performance: switch ≈ tens of ns, the tick
  check ≈ 1–2 % [I], no parallel speed-up, a helper hand-off (~10–50 µs) per blocking syscall
  only when other threads exist; GC: a root-table patch in C, no change to collection.

### 3b. One OS thread per Java thread with a global interpreter lock

- **Mechanics.** Scala Native's own thread stack runs unchanged: `scalanative_GC_pthread_create`
  from a multithreaded Immix build, `PosixThread`'s pthread mutex/condvars, `poll`-based sleep.
  The VM adds a GIL held while interpreting, released around `@blocking` externs (they include
  `pthread_cond_wait`, so `park` works as written) and every N ticks for fairness (CPython's
  eval breaker, MRI's timer thread) [V that the externs are marked; I for the rest].
- **GC.** Immix must be built with `SCALANATIVE_MULTITHREADING_ENABLED` (and without trap
  yield points): each OS thread registers as a mutator, and the collector's
  `Synchronizer_acquire` waits until every other mutator is at a safepoint [V]. Under a GIL
  every non-running thread is either inside a blocking extern or waiting for the GIL, so the
  VM must call `scalanative_GC_set_mutator_thread_state(Unmanaged)` before both waits and
  `(Managed)` after; then the collector never waits [I]. Each thread's frame stack becomes a
  per-mutator root (a pointer in `ThreadInfo` or the `MutatorThread`); native stacks and
  registers are scanned by the existing `Marker_markProgramStack` [V].
- **`&mut Vm`, callbacks.** `Vm` holds `Rc`, `Cell` inline caches and `thread_local!` tables;
  handing it between OS threads means `unsafe impl Send` reasoning for the whole VM, a
  process-global callback table with a lock (today's `TABLE`/`RUNNING` are per OS thread,
  `callback.rs:21-25`), and per-thread `errno` comes for free [V/I]. A non-`@blocking` extern
  that blocks (or a long C computation) holds the GIL and stalls everyone.
- **Trade-offs.** Robustness: medium (real OS threads plus a global lock, lock hand-off
  bugs, C libraries' thread state, `Rc` across threads); effort: at least (a) plus the
  Immix multithreading integration and the `Send` audit [I]; performance: GIL convoy on
  every hand-off, still no parallelism; GC: Immix's own synchronizer, more C code in play.
  It is the right base for real OS threads later, but not the cheapest route to green
  semantics.

### 3c. A non-recursive interpreter (explicit frame stack)

- Today one interpreted call costs one Rust `run` frame; a switch therefore needs a native
  stack per thread. With an explicit stack of `(Rc<Code>, pc, slot base, unwind handler)` and
  `Ret` popping instead of returning, a thread's whole interpreted state lives in its frame
  stack and a switch is a pointer swap. Files to rewrite: `exec.rs`, `invoke.rs`, `unwind.rs`,
  `shortcut.rs`, parts of `compute.rs` (~700 lines) [V].
- It does not remove native stacks: `vm.call` is re-entered from Rust natives
  (`runtime.rs`, `services.rs`, `linktime.rs:127`, module init, `describe_throwable`, the
  weak-reference handler) and from C callbacks (`callback.rs:56`) [V]. A thread that parks
  below such a frame cannot be switched without a coroutine (Lua's "attempt to yield across
  a C-call boundary"; CPython 3.11 kept C-level re-entry recursive; Loom pins the carrier).
  So (c) reduces per-thread native stack use but still needs (a)'s coroutines for the
  re-entrant cases, or a rule that natives never park.
- Effort 2–4 weeks with a performance risk either way (CPython gained speed from it, but our
  `run` keeps `code` and `slots` in registers) [I]. Worth doing later for stack traces and
  cheap threads, not as the first step.

### 3d. Prior art

- **Go**: goroutines on their own stacks; a blocking syscall hands its OS thread off so other
  goroutines keep running — the model for (a)'s helper threads.
- **Loom**: virtual threads copy frames to the heap on unmount and pin the carrier inside
  native frames; Scala Native 0.5.12 does the same with `delimcc` (`nativelib/.../delimcc.c`,
  `javalib/src/main/scala-3/java/lang/VirtualThread.scala`) [V]. The VM cannot copy its Rust
  frames, so virtual threads become green threads (each boundary a coroutine) once (a) exists.
- **CPython / MRI**: OS threads plus GIL/GVL, released around blocking calls and by a timer;
  their well-known convoy effects are the cost of (b).
- **Lua**: one `lua_State` per coroutine with its own Lua stack; yields across C frames need
  continuations (5.2 `lua_callk`) — the same boundary problem as (c).
- **GraalVM/Espresso**: Java threads are host threads and the host GC scans everything; only
  applicable once we have OS threads and a collector that stops them (b).

## 4. Recommendation and plan

### 4.1 Recommendation: option (a), with VM-owned monitors and helper threads for blocking C

Green threads on stackful coroutines give the concurrent test suites what they need
(interleaving, timed waits, interrupts, parks) without touching the memory model, the C
runtime's threading or the callback design, and they are the base for virtual threads. Two
choices inside (a):

- **Monitors in the VM, not the lock word.** Reusing `BasicMonitor`/`ObjectMonitor` would
  require the multithreaded object header (+8 bytes per object and array, the multithreaded
  Immix build for `ObjectHeader.h`, new `layout.rs` constants, a full re-run of the 8,300
  tests) and pays a CAS through the FFI per `synchronized`. Instead the VM overrides
  `enterMonitor`, `exitMonitor`, `getMonitor` (throws if ever reached), `_Object.__wait*`,
  `__notify*` and `NativeThread.holdsLock` with a side table `address → { owner, recursion,
  entry queue, wait set }` that drops an entry once it is unowned and empty. Addresses are
  stable (Immix never moves) and an object with a live entry is always reachable from a
  frame, so the table never holds a dangling address [V/I]. About 200 lines of Rust.
- **Blocking externs on helper OS threads**, inline when the program has one thread.

### 4.2 Step-by-step plan

1. **Per-thread state** (no behaviour change). Move `frames`, the `thread` cell, `stack_floor`,
   `overflowing` and the trace stack from `Vm`/`Funcs` into a `GreenThread`; `Vm` keeps
   `threads` and `current`. Make `Frames::new` take its reservation size. `just gates` passes.
2. **Coroutines + GC roots.** Add `corosensei`; run the main thread as a coroutine too. Replace
   `scalai_frame_stack` in `gc_glue.c`/`Marker.c` with a table of `{native lo, native hi,
   frames lo, frames top}` per thread, and set `currentThreadInfo` and the mutator's
   `stackBottom` at every switch. Per-coroutine `StackOverflowError` floor from the
   coroutine's bounds instead of `stack::bounds()`. `gcstress`, `overflow` and the language
   tests pass unchanged.
3. **Scheduler and runtime overrides** — the first milestone that un-skips most tests. Run
   queue, timer heap (sleep, `parkNanos`, `parkUntil`, timed waits), park permits, tick
   preemption at back-edges and calls, `Thread.yield`. Override
   `scalanative_GC_pthread_create` (spawn a green thread that calls the routine through the
   existing callback path), `PosixThread.park/unpark/sleep/sleepNanos/interrupt/setPriority/onTermination`,
   `PosixThread$.yieldThread/defaultOSStackSize`, the `MemoryLayout$*$` offset accessors,
   `isContinuationsSupported` → `false`, and the monitor methods of 4.1. Set
   `isMultithreadingEnabled` to `true`; call `runtime.onShutdown()` after `main`. Expected
   result: the 80 `java.util.concurrent` classes, `PriorityQueueTest`, `StackOverflowTest`'s
   three cases and `ModuleInitializationTest` run; most of the 2,827 methods should pass,
   with failures concentrated in tests that measure timing or rely on a spinning thread being
   descheduled quickly [I].
4. **Blocking externs off-thread.** Helper pool, `errno` copy-back, inline fast path. This
   fixes `ServerSocketTest.closeWithActiveAccept` (subject to `close` actually waking a blocked
   `accept` on macOS/Linux — Scala Native's own suite passes it with OS threads, so the
   javalib `close` path is known to work) and lets `ProcessTest` finish without timeouts.
5. **Later.** Virtual threads: implement `Continuations.boundary/suspend/resume` on green
   threads and flip `isContinuationsSupported`. Stack traces from the frame stack (roadmap item
   4) fit the per-thread frame stacks. Real OS threads (option b) remain the parallelism step
   and reuse the per-thread state of step 1.

### 4.3 ADR 0003

Keep the decision; amend the consequences:

1. "Blocking I/O … must be turned into non-blocking waits the scheduler drives" → blocking
   externs (`Attr.Extern(blocking)`) run on helper OS threads while the scheduler continues;
   timers and parks are scheduler waits; a program with one thread calls them inline.
2. The C runtime stays the single-threaded Immix build; only its root scan learns about
   per-thread native and frame stacks (extends ADR 0004's patch list).
3. The object header keeps no lock word; monitors, `wait`/`notify` and `holdsLock` are
   implemented in the VM, and `MemoryLayout`'s offset accessors are overridden.
4. Java virtual threads are reported unsupported (`isContinuationsSupported = false`) until
   they are built on green threads; Scala Native's `delimcc` continuations are not used.
5. Preemption: a tick counter at loop back-edges and calls, in addition to the switch points
   ADR 0003 lists.

## Sources

- `ref/scala-native` v0.5.12: `nativelib/src/main/scala/scala/scalanative/runtime/{NativeThread,
  GC, Object, Blocker, VirtualThread, MemoryLayout, package}.scala`, `runtime/monitor/*.scala`,
  `meta/LinktimeInfo.scala`, `nativelib/src/main/resources/scala-native/{nativeThreadTLS.[ch],
  delimcc.[ch]}`, `gc/immix/{ImmixGC,Synchronizer,MutatorThread,Heap,Marker}.c`,
  `gc/immix_commix/headers/ObjectHeader.h`, `gc/shared/ThreadUtil.c`,
  `javalib/src/main/scala/java/lang/{Thread,ThreadLocal}.scala`, `java/lang/impl/PosixThread.scala`,
  `java/util/concurrent/locks/LockSupport.scala`, `java/lang/process/*.scala`,
  `java/net/AbstractPlainSocketImpl.scala`, `javalib/src/main/scala-3/java/lang/VirtualThread.scala`,
  `clib/.../stdatomic.{scala,c}`, `nscplugin/src/main/scala-3/.../{NirGenExpr,NirGenStat,NirDefinitions}.scala`,
  `tools/src/main/scala/scala/scalanative/codegen/{Lower,CommonMemoryLayouts,Metadata}.scala`,
  `codegen/llvm/AbstractCodeGen.scala`, `build/{LLVM,NativeLib,NativeConfig}.scala`,
  `docs/contrib/gc-safepoint-trampoline.md`, `unit-tests/shared/src/test/scala/utils/AssumesHelper.scala`,
  `.../javalib/lang/{ProcessTest,StackOverflowTest}.scala`, `.../javalib/net/ServerSocketTest.scala`.
- This repository: `crates/vm/src/{lib,stack,heap,linktime,layout}.rs`,
  `interp/{mod,exec,invoke,ops}.rs`, `natives/{mod,runtime,callback,llvm,ffi/mod}.rs`,
  `crates/nir/src/nir.rs`, `crates/snrt/build.rs`, `native/scalai/gc_glue.c`,
  `native/nativelib/scala-native/gc/immix/Marker.c`, `docs/adr/000{2,3,4}*.md`,
  `docs/conformance.md`, `tmp/sn-report/*.txt`.
- `corosensei` 0.3.4 documentation (docs.rs), fetched 2026-09-27.

