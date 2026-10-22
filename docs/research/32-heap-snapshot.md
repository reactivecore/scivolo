# 32 — Heap snapshot of the initialized runtime (#9)

Date: 2026-10-22. Design and plan for `run_main` from a snapshot of the runtime modules the
`scala.scalanative.runtime.package$.init` call constructs, stored beside the library image
(ADR 0005). Claims are [V] verified by running or reading, [I] inferred. Paths: `V/` =
`crates/vm/src/`, `N/` = `ref/scala-native/nativelib/src/main/scala/scala/scalanative/runtime/`,
`J/` = `ref/scala-native/javalib/src/main/scala/java/lang/`, `G/` =
`native/nativelib/scala-native/gc/immix/`. Measured on this machine (Apple M4, macOS 15, release
build of main c8c1b5f0, library image warm, `SCIVOLO_CACHE=tmp/cache`), which is not the M4 Pro
reference machine of `docs/performance.md`; the system was loaded by other sessions (medians
40% above minima), so minima are reported. Method: temporary timers (not committed) around
`Vm::new`, `init_runtime`, `main`, `shutdown_runtime`, `World::defn`, `lower_code` and
`Vm::module`, printed at exit; `just bench NAME 20` for wall times. Follows research 03 §3(c),
which chose this step, and the findings on #9 (2026-09-29).

**Recommendation.** Take the snapshot as a verbatim dump of one memory region mapped at a fixed
address, in which a training VM does *all* its allocations until `init` returns; restore it by
mapping the same address, which keeps every object address and so every address-based identity
hash (§3). Only words pointing out of the region (C trampolines, `dlsym`ed addresses) are
relocated, by name; anything else outside the region refuses the snapshot and the run starts
cold. The VM's tables are stored by name and re-interned (§2.2); lowered code is not stored.
Immix is untouched: the region is one permanent root range (§4). Keyed by the image key, the
build and the platform, written by the first cold run, pruned with the image (§5). Expected:
hello 8.3 → ~4.2 ms in-process, ~7 ms wall (today 11.1 ms); moderate 16.9 → ~12.5 ms (§7).
Five MRs of ≤ 400 lines (§8); ADR 0034 records the decisions (§10).

## 1. Where the time goes today

In-process phases of the fastest of six runs (`T process start to exit` is the VM thread's view;
`exec`, dyld, the 1 GiB interpreter thread and exit add ~2.8 ms to the wall time) [V]:

| phase | hello | moderate | what it is |
|---|---:|---:|---|
| map image | 0.89 | 1.01 | `libs::image`, `Image::open` (`crates/libimage/src/image.rs:58-72`) |
| `load_runtime` | 0.44 | 0.46 | `dlopen` of libsnrt (`V/natives/ffi/dl.rs:10-23`) |
| `Heap::new` | 0.07 | 0.07 | `scivolo_GC_init` (`V/heap.rs:84-85`) |
| `bootstrap` | 0.22 | 0.23 | `java.lang.Class`, `String`, `CharArray` linked (`V/rtti.rs:67-91`) |
| `init_runtime` | **4.18** | **4.40** | `runtime.package$.init` (`V/session.rs:123-128`): 91 modules, 246 classes, 554 functions; decode 2.11 ms (791 defs), lower 1.15 ms (525 methods), ~0.9 ms executing and linking |
| `main` | 1.55 | 9.75 | hello: +13 modules, 217 defs, 166 methods; moderate: +59 modules, 1,221 defs, 954 methods |
| `shutdown` | 0.56 | 0.58 | `onShutdown` (`N/package.scala:94-130`) |
| total in-process | 8.28 | 16.87 | |
| wall, `just bench` min / median | 11.1 / 16.4 | 20.3 / 29.1 | |

`init_runtime` constructs the same 91 modules for every program [V both]; the costly ones by
inclusive time (hello trace; 5.07 ms in that loaded run, 2.24 ms in the quieter moderate trace
of the same 91 modules): `java.lang.Thread$MainThread$` (contains `ClassTag$`/`Manifest$`/`ManifestFactory$` 1.4 and `impl.PosixThread$`
1.2), `scala.Console$` 0.6, `java.nio.charset.CoderResult$` 0.4, `java.lang.Streams$` 0.3,
`NativeExecutionContext$` 0.17, `ThreadGroup$` 0.2, `WeakReferenceRegistry$` 0.2. Their cost is
mostly decoding and lowering their initializers, not running them.

Heap after `init` (both programs): **permanent heap 60,832 B** (module instances, rtti,
literals, constants), **collected heap 9,618 B in 114 objects** [V]. After hello's main: 72.8 KB
and 28 KB in 197 objects; after moderate's: 124 KB and 79 KB in 1,421 objects. VM tables after
`init`: 91 modules, 246 classes, 23 string literals, 299 vtable entries, 6 link-time properties,
19 link-time conditions, 1 C trampoline (the weak-reference callback), 0 globals, 0 constants.

So the snapshot is ~70 KB of memory plus tables; the gain is bounded by `bootstrap` +
`init_runtime` = 4.4–4.6 ms in-process, and moderate's remaining cost is the decoding and
lowering of what *main* uses (§7, §9).

## 2. What must be captured

### 2.1 Memory

Everything the VM allocates lives in two places (`V/heap.rs:1-9`):

1. **Permanent heap** (`Heap::permanent`, `V/heap.rs:129-151`): 1 MiB chunks from the Rust
   allocator, registered as conservative root ranges (`scalanative_GC_add_roots`, `:149`).
   Holders, by allocation site: module instances (`V/interp/dispatch.rs:45`), rtti objects with
   a trailing `ClassId` word (`V/rtti.rs:100-103`), interface tables (`:139-142`),
   `refFieldOffsets` `i32` arrays without header (`V/gcinfo.rs:87-91`), class-name strings
   (`:155-161`), string literals (`V/object.rs:91-102`), `argv` (`:105-116`), global `var`s and
   `const`s (`V/interp/value.rs:83-87`), constant data (`:142-145`), thread cells (`V/lib.rs:85`,
   `V/threads/spawn.rs:25`), LLVM-intrinsic scratch cells (`V/natives/llvm.rs:127`), JUnit
   C strings (`V/natives/junit.rs:42`), link-time property strings (`V/linktime.rs:68`).
   Objects and arrays carry a header; the raw blobs do not, so only the VM knows their shape.
2. **Immix heap** (`Heap::object`/`array`, `:118-126`): everything else, 114 objects after
   `init`, reachable from the permanent heap (e.g. `Streams.out`, `MainThread`'s fields).

Design (§3): a training VM allocates both in one region; the snapshot is the region's used
prefix, verbatim. Nothing is traversed or compacted at restore; training garbage (a few KB)
stays.

### 2.2 VM tables (`V/lib.rs:50-76`)

| field | stored | how |
|---|---|---|
| `modules` (91) | yes | module name → address; re-interned through `World::top` |
| `classes.all` (246), `by_name` | yes, in id order | name, kind, parent, traits, size, `array_elem`, rtti address, fields as (member signature, offset, NIR type); `ClassId`s are baked into rtti objects (`V/rtti.rs:102`, `:196-198`), so the order must be reproduced exactly |
| `classes.vtables` (299) | no | rebuilt lazily (`V/interp/dispatch.rs:93-109`) |
| `strings` (23) | yes | text (UTF-16 units for lone surrogates) → address |
| `rtti` counters (`meta`, `meta_size`, `next_class`, `next_trait`, `strings_ready`) | yes | `V/rtti.rs:55-64`; `bootstrap` is thereby replaced |
| `gc_classes` (`boxed_ptr`, `weak`, `next_weak`, `weak_referent`, `weak_handler`) | yes | `V/gcinfo.rs:17-24`; `weak_handler` as a function name; restore calls `update_gc_ids` and `watch_weak_refs` (`:35-38`) |
| `linktime.props` (6) | yes | name → value (strings are region addresses) |
| `linktime.conds` (19) | no | keyed by `&Cond` addresses inside `Rc<Defn>` (`V/linktime.rs:78-80`), process-specific |
| `callbacks.owned` (1) | yes | (slot, function name); the thread-local `TABLE` (`V/natives/callback.rs:21-25`, ADR 0002) is refilled, and `func()` resolves the name on first call |
| `thread` cell | yes | its region address; `run_threads` reads the native thread from it (`V/threads/mod.rs:91`) |
| `funcs` (`all`, `by_name`, `inlinable`, `boxes`, `proxies`, `instantiators`), `Code` | no | lazily rebuilt; code bakes module addresses (`V/interp/lower/constants.rs:18`) and `FuncId`s, which stay valid only because the region keeps addresses, but a code snapshot is a later step (§9) |
| `funcs.globals`, `consts` | yes if non-empty | name → address (0 after `init` today) |
| `world` (`loaded`, `files`, `places`, `defns`, `members`) | no | lazily rebuilt from the image; the restored VM decodes only what `main` needs |
| `threads` (`sched`, `monitors`, `by_native`, `initializing`) | no | the training's main green thread has ended; `initializing` must be empty and `monitors` (keyed by address, `V/threads/monitors.rs:12`) empty, else the writer refuses |
| `variadics`, `resources`, `dlerror`, `scratch`, `junit`, `quotes` | no | process-specific or empty |

Function references in heap words: `func_ref(id)` carries the tag `0xF1C0` in the top 16 bits
(`V/interp/funcs.rs:12-13`), so the writer can recognize them in `Ptr`-typed fields and store
the function's name; the reader registers the function (`Vm::func`) and rewrites the word.

### 2.3 State outside `Vm`

- **Immix** (`G/State.c:5-11`: `heap`, `blockAllocator`, `customRoots`, `weakRefStack`,
  `currentMutatorThread`; `G/WeakReferences.c:10-11`; `native/scivolo/gc_glue.c:13-34,77`): not
  stored; the restoring process runs `scivolo_GC_init` as today and the region is added as a
  root range (§4).
- **Callback table** (ADR 0002) and **SIGINT counter** (ADR 0013, `V/natives/ffi/sigint.rs:5`):
  refilled / untouched.
- **C library state inside heap bytes**: `PosixThread` keeps `pthread_mutex_t`/`pthread_cond_t`
  in a Scala byte array (`J/impl/PosixThread.scala:39-59`) and its companion keeps
  `pthread_condattr_t`/`pthread_mutexattr_t` the same way (`:391-403`); the main thread's
  `handle` is 0 (`:61-62`). Those bytes are inert in the VM, which replaces park, unpark, sleep,
  monitors and `wait`/`notify` (`V/natives/threads.rs:17-68`, ADR 0003); copying them is safe
  on macOS and glibc, where these types hold no pointers to themselves [I].
- **File descriptors**: `FileDescriptor.in/out/err` are the numbers 0–2 in an
  `AtomicObjectHandle` (`ref/scala-native/javalib/.../io/FileDescriptor.scala:18-35,71-83`),
  valid in every process. Nothing is written during `init` [V: no output].
- **Pointers into process-specific memory**: C trampolines (`V/natives/callback.rs:68-78`, in the
  VM binary, ASLR-slid per process), `dlsym`ed functions and variables (`V/natives/ffi/dl.rs:27-40`,
  libsnrt loaded at a random address), Rust heap (`V/natives/resources.rs:23-34` hands `Vec`
  pointers to the program), C `malloc`, the frame stack (`stackalloc`). The writer classifies
  every `Ptr`-typed word (§3); trampolines and libsnrt symbols are relocated by name
  (`dladdr` names the symbol of an address inside libsnrt [I]), the rest refuses the snapshot.
  `Long` fields that hold an address cannot be told from integers; accepted [I] (none found in
  `init`'s modules by reading: `AtomicObjectHandle.value` holds file descriptors).

### 2.4 Threads

The training's main green thread (`V/threads/mod.rs:89-93`) has returned when the snapshot is
written; its frame stack and coroutine are gone. The restored VM spawns a fresh main thread for
`run_main` whose `Thread`/`NativeThread` come from the stored cell (`V/natives/runtime.rs:9-10`,
`:31-36`). `MainThread` has `tid = 0` (`J/Thread.scala:562-574`) and the thread registry is keyed
by that `Long` (`N/NativeThread.scala:170-178`), not by identity.

### 2.5 Environment-dependent initialization

`init` itself (`N/package.scala:57-89`) writes `ExecInfo.filename`, `ExecInfo.startTime` and
builds `args` from `argv`: **it is re-run at restore with the real `argv`**. The second run is
idempotent in the VM: `setupCurrentThreadInfo` and `StackOverflowGuards.setup` are no-ops
(`V/natives/runtime.rs:12-17`), `Thread.currentThread()` returns the stored `MainThread`
(`J/Thread.scala:584-586`), and the three assignments overwrite [V by reading].

Of the 91 modules `init` constructs (listed by the temporary module timer; the first is
`runtime.BoxedUnit$`, the last `scala.scalanative.ffi.time$`), none reads the environment, time or process identity [V by name and source]: the runtime's own
objects, `Thread$`/`MainThread$`/`ThreadGroup$`/`PosixThread$`, `Streams$`/`FileDescriptor$`,
`Console$`, charsets (`Charset.defaultCharset()` is the constant `UTF_8`,
`ref/scala-native/javalib/.../charset/Charset.scala:69`), `ClassTag$`/`Manifest$`, boxes,
`Nil$`/`None$`/`Predef$`, `WeakReferenceRegistry$` (registers the collector callback,
`.../ref/WeakReferenceRegistry.scala:95-96`; restored through `gc_classes.weak_handler`).
`ThreadStackSize` (`N/NativeThread.scala:79-87`, reads `SCALANATIVE_THREAD_STACK_SIZE`) is not
constructed by `init`.

Modules that **must never be in a snapshot** (the deny list the writer enforces by name, for
when the training set grows, §8 S5): `java.lang.System$SystemProperties$` (env `TMPDIR` etc.,
`J/System.scala:284-294,361-403`), `java.lang.EnvVars$` (copies `environ`, `:490-508`),
`java.util.Locale$`/`TimeZone$`, `java.util.Random` instances and
`java.util.concurrent.ThreadLocalRandom$` (time seeds, `.../util/Random.scala:44`,
`.../concurrent/ThreadLocalRandom.scala:115`), `scala.util.Random$`, `java.lang.Runtime$`
(shutdown hooks), `java.lang.ProcessBuilder`, `java.net.InetAddress$`, `java.io.File$`
(`user.dir`), `java.util.concurrent.ForkJoinPool$` (processor count), `scala.scalanative.runtime.StackTrace$`.
hello and moderate construct none of them [V]; moderate's `f"…"` goes through
`java.util.Formatter$`/`FormatterImpl$` without `Locale` [V].

## 3. Addresses: a fixed region, no relocation inside

`System.identityHashCode` is the object's address (`J/System.scala:41-43`) and
`Class.hashCode` likewise (`N/Class.scala:117-118`); any hash table keyed by identity during
training (e.g. `ClassTag`'s cache, `java.util.HashMap` with default `hashCode`) is corrupted
if its keys move. Relocating the snapshot to a new address is therefore unsafe in general.

Decision: the training VM maps a **region** (64 MiB reserved, `MAP_NORESERVE`) at a fixed hint
address before any allocation and serves every `Heap::permanent`, `object` and `array` call from
it by bump allocation (zeroed; headers written as `alloc_object_in(Permanent)` does,
`V/object.rs:21-50`). The dump is `[region.start, high_water)` with the final addresses already
in place. The restoring VM maps the same hint, checks it got that address, reads the file into
it (18 pages; a file mapping would fault page by page, which #9's findings measured as the
remaining image cost), and registers it as one root range. If the hint is taken, the run starts
cold — never relocated. The permanent allocator continues after the high-water mark in the same
region for the rest of the run; Immix serves `object`/`array` again.

Probe on this machine (a 15-line C program: `mmap(hint, 64 MiB, PROT_READ|PROT_WRITE,
MAP_PRIVATE|MAP_ANON)` without `MAP_FIXED`, compared with the hint, in 30 fresh processes;
not committed): `0x6_0000_0000`, `0xA_0000_0000`, `0xE_0000_0000`, `0x2000_0000_0000`,
`0x5000_0000_0000` and `0x7E00_0000_0000` were granted 30/30; `0x10_0000_0000` was moved every
time [V]. Hint per platform: macOS `0x2000_0000_0000`, Linux x86-64 `0x5000_0000_0000` [I,
between the brk heap and the mmap area; CI measures it]. The region must be mapped before
`Heap::new`, so Immix's own `mmap(NULL, …)` (`G/Heap.c:48,123`,
`native/nativelib/scala-native/gc/shared/MemoryMap.c:47`) cannot land on it.

Words pointing **outside** the region are the only relocations, found precisely: for an object,
`refFieldOffsets` gives reference fields (always inside); `Ptr`/`RawPtr`-typed fields
(`Type::Ptr`, excluded from `refFieldOffsets` at `V/gcinfo.rs:80-82`) are classified; arrays by
element type; raw permanent blobs by a **kind** the allocator records in training mode
(`Object`, `PtrArray`, `I32Array`, `Bytes`, `Typed(NIR type)`), one push per allocation at the
13 `permanent` call sites in the eight files of §2.1. Classification of a non-null `Ptr` word: inside the region → keep;
a trampoline address → `Trampoline(function name)`; inside libsnrt → `Symbol(name)` via
`dladdr`; a `func_ref` → `Function(name)`; small integers (file descriptors, sizes) → keep;
anything else (Rust heap, C heap, frame stack, VM binary data) → **refuse**, with the owning
module named under `SCIVOLO_TRACE`.

Alternative kept in reserve: position-independent dump with a relocation bitmap (pdumper
style) plus an identity-hash side table in the VM (hash assigned on first use, stored and
relocated with the snapshot), which removes the fixed-address dependency at the cost of a map
lookup per `identityHashCode` [I]. Not needed while the hint is granted.

## 4. Immix (ADR 0004)

- The region is outside Immix's heap range (`Heap_IsWordInHeap`, `G/Heap.h:27-29`), registered
  through `scalanative_GC_add_roots` (`G/ImmixGC.c:116-118`) as the permanent chunks are today;
  `Marker_markCustomRoots` scans it conservatively each collection (`G/Marker.c:220-238`): ~70 KB
  per collection, under 10 µs [I]. Objects in it are never freed; their references into the new
  Immix heap are roots.
- No block, line, bytemap or allocator cursor is serialized; the training's Immix heap is
  unused (allocation is redirected), so the snapshot carries no collector state. The
  collector's process-global state is initialized by `scivolo_GC_init` in every process as
  today (`V/heap.rs:63-87`, one VM per process).
- Class ids: `GcIds` (`V/heap.rs:49-58`) are re-set from the stored counters by
  `update_gc_ids` (`V/gcinfo.rs:94-107`); weak-reference ids continue from `next_weak`.
- Weak references in the region are scanned as roots, so their referents are never cleared
  [I]; after `init` the registry's `weakRefsHead` is null [I], so none exist.
- Thread roots (`scivolo_threads`, `gc_glue.c:52-55`) are published per run as now
  (`V/threads/roots.rs:12-28`).

## 5. Image key and invalidation

- File `snap-<image key>-<BUILD>-<os>-<arch>.bin` in the cache directory beside
  `libs-<key>.img`. The image key covers the jars, the program version and the tag
  (`crates/libimage/src/build.rs:14-28`); `BUILD` hashes the crate sources, manifests, lockfile
  and `libs/` (`crates/libs/build.rs:1,25`), which covers the VM, the overlay NIR (ADR 0009)
  and libsnrt; the platform fixes the hint and pointer layout. A header repeats key, `BUILD`,
  hint, length, the training version and the deny-list version; a mismatch or a short file
  starts cold.
- Written by the **first cold run** after `init` returns (every cold run is a training run,
  since allocation goes to the region until `init` returns): staging file + rename as the image
  build does (`crates/libimage/src/build.rs:32-44`), skipped when the cache is read-only, when
  `SCIVOLO_NO_SNAPSHOT` is set, or when the writer refuses (§3). Cost once: a typed walk of
  70 KB and a write, ~1 ms [I].
- Pruned with the image: `cache::prune` on `snap-*` with `IMAGE_UNUSED`
  (`crates/libimage/src/cache.rs:10,51-64`); each use touches the file.
- **User classpath shadowing**: user roots precede the image (`V/classpath.rs:37-39`), so a
  user jar could redefine a snapshotted class. The restore checks the user roots for any name in
  the snapshot's package set (`java/lang/`, `scala/`, `niocharset/`, …, stored in the header):
  jars by their index, directories by one `read_dir` of each top package [I, ~20 µs]; a hit
  starts cold and does not write.
- `SCIVOLO_NO_SNAPSHOT=1` runs cold without writing, for A/B tests and bisecting.
- The REPL (`start_runtime`, `V/session.rs:30-34`), scripts and the macro `rerun`
  (`crates/scivolo/src/main.rs:118-130`) all pass through `Vm::new` + `init_runtime`, so they
  all gain.

## 6. Precedents (acknowledges.md:83-86; research 03 §3(c))

| system | what it does | taken |
|---|---|---|
| Dart AppJIT / app snapshots | a training run, then the isolate heap (and code) serialized in clusters; loaded by the same VM build | training run = our `init`; deferring code to a later step |
| Julia sysimage / pkgimages | a dumped session as a shared library, pinned to the machine and build, relocation lists for pointers to the runtime | pinning by `BUILD`, platform and image key; relocation only for runtime symbols, by name |
| GraalVM Native Image heap | static initializers run at build time only when provably safe, the rest at run time; `RecomputeFieldValue` for environment-dependent fields | deny list and refusal on unclassifiable pointers; `init` re-run for `ExecInfo`/`args` |
| Emacs pdumper | a walk of the heap with a relocation table, loadable only by the dumping binary | the typed walk; `BUILD` in the key. Our region keeps addresses, so the relocation table is not needed |
| V8 startup snapshot | a context snapshot after running the embedder's script, deserialized in object order | the `SCIVOLO_NO_SNAPSHOT` switch and the A/B gate over all test programs |

## 7. Expected gains

From §1, all [I] until measured: restore ≈ 0.02 ms read + ~0.2 ms re-interning ~1.5k names
(91 modules, 246 classes with ~1,100 fields, 23 literals) + 0.05 ms `init` re-run.

| | hello | moderate |
|---|---:|---:|
| today, in-process | 8.28 | 16.87 |
| − `bootstrap` + `init_runtime` | −4.40 | −4.63 |
| + restore | +0.3 | +0.3 |
| after, in-process | ~4.2 | ~12.5 |
| after, wall (today 11.1 / 20.3 min) | ~7 | ~16 |

Hello meets the target (< 10 ms) with margin on this machine and should land near 5 ms wall on
the M4 Pro [I]. The remaining hello cost is the image map (0.9), libsnrt (0.4), main's own
decoding and lowering (1.4) and `onShutdown` (0.6). Moderate stays dominated by main's decoding
(≈4.3 ms) and lowering (≈3 ms) of 1,221 definitions; a snapshot of decoded definitions and
lowered code is the next lever after this one (§9), not part of #9. Widening the training set
(S5) with the pure collection and formatting modules moderate constructs at depth 0
(`immutable.HashMap$` 0.26, `MapNode$` 0.19, `RyuDouble$` 0.13, `Ordering$Int$` 0.06,
`EmptyArrays$`, `ArraySeq$`, `List$`; `Console$` 0.6 and `CoderResult$` 0.4 are already in
`init`'s set) saves ~0.7 ms more on such scripts [V sizes, I gain].

## 8. Plan

Each row one MR, sizes are Rust lines of diff [I]; every MR passes `just gates` and
`crates/scivolo/tests/programs.rs` both with and without `SCIVOLO_NO_SNAPSHOT`.

| step | scope | files | size | test | after |
|---|---|---|---|---|---|
| S1 region allocator | `Region` (fixed-hint `mmap`, bump allocation, high-water mark) in `V/heap.rs`; `Heap` training mode routing `permanent`/`object`/`array` to it and recording allocation kinds; the eight permanent call sites pass their kind; `Vm::new(classpath, Snapshot::Train)` | `V/heap.rs`, `V/heap/region.rs` (new), `V/object.rs`, `V/rtti.rs`, `V/gcinfo.rs`, `V/interp/value.rs`, `V/lib.rs` | ~250 | unit: hint granted on CI (both platforms), allocations land in the region; programs unchanged | — |
| S2 writer | typed walk of the region (objects by rtti, blobs by kind), pointer classification with `dladdr`/trampoline/`func_ref` cases and refusal, deny list, table serialization by name (§2.2), file format with header; `scivolo --write-snapshot FILE` (hidden) | `V/snapshot/walk.rs`, `V/snapshot/write.rs`, `V/snapshot/format.rs` (new), `crates/scivolo/src/main.rs` | ~400 | the walk finds exactly the 114 + permanent objects of `init`; a module storing a Rust-heap pointer refuses; format round-trips | S1 |
| S3 reader | map at the hint, read, verify header, register roots, rebuild tables (re-intern names, `classes.all` in order, rtti and gc counters, props, callbacks, thread cell), `init` re-run; `Vm::new(classpath, Snapshot::Restore(file))` with cold fallback | `V/snapshot/read.rs` (new), `V/lib.rs`, `V/session.rs` | ~400 | every `tests/programs` program gives the same output restored and cold (`programs.rs` runs both); a corrupt file falls back; measured hello/moderate in-process | S2 |
| S4 cache integration | key, file name, first-run write with staging, prune, shadowing check, `SCIVOLO_NO_SNAPSHOT`, `docs/performance.md` numbers | `crates/scivolo/src/snapshot.rs` (new), `crates/libs/src/lib.rs`, `crates/libimage/src/cache.rs` | ~200 | two concurrent first runs; a stale key is ignored; a shadowing `--cp` starts cold; `just bench hello` | S3 |
| S5 wider training (optional) | a training program constructing the pure collection/format modules of §7, measured on moderate and `tests/programs`; deny list exercised | `V/snapshot/train.rs` (new) | ~100 | moderate gain ≥ 0.5 ms, else dropped | S4 |

S1 and S2 can be developed together on one branch but reviewed separately; S3 depends on S2's
format. The code snapshot (§9) is a separate issue after S4.

## 9. Risks and open points

- **Hint not granted** on some machine or under a sanitizer: cold start, no gain, no
  incorrectness. CI records the grant rate; a second hint per platform is tried before giving up.
- **Pointers out of the region** in a future Scala Native runtime (e.g. `init` storing a
  `malloc`ed buffer): refused at write time, cold start; the diagnostic names the module.
- **Identity hashes** are preserved only because addresses are; S3's A/B test over
  `tests/programs` (which includes hash-heavy collections) is the guard. If a relocation is ever
  needed, the side table of §3 is the fallback.
- **`Long` fields holding addresses** are invisible to the classifier [I]; none in `init`'s
  modules by reading.
- **Environment drift** between training and use: everything `init` computes from the
  environment (`argv`, start time, file name) is recomputed; the deny list covers the rest and
  must be reviewed when the runtime version changes (ADR decision 3 of ROADMAP).
- **Lowered code keeps `FuncId`s and module addresses** (`V/interp/lower/constants.rs:18`);
  not stored now, so no risk; a code snapshot must store functions by name.
- **Conservative scan of the region** on every collection: 70 KB today, more with S5; keep the
  training set small.
- **The 1 MiB permanent chunks** of today become region allocations; a program allocating many
  permanent objects (REPL sessions) must not exhaust 64 MiB: the allocator falls back to chunks
  beyond the high-water mark, as now.
- **Measurement noise**: this machine was loaded; the gains in §7 are arithmetic on minima, to
  be re-measured in S3 on the M4 Pro.
- **Moderate's target** is not reached by #9 alone: decoding and lowering of main's 1,221
  definitions remains (~7 ms). A snapshot of `World::defns` and `Code` keyed the same way is the
  follow-up (Dart AppJIT includes code for this reason).

## 10. ADR 0034

Needed, reserved number 0034: "Heap snapshot in a fixed region". Decisions it records: (1) the
snapshot is a verbatim dump of a fixed-address region in which the training VM allocated
everything, never relocated — cold start otherwise; (2) all snapshotted objects are permanent and
one Immix root range; (3) the training set is what `runtime.package$.init` constructs, `init`
is re-run, and a deny list plus pointer classification refuse unsafe snapshots; (4) the file is
keyed by image key, `BUILD` and platform, written by the first cold run and pruned with the
image; (5) VM tables are stored by name, code is not. Consequences: a new kind of `unsafe` in
`heap/region.rs` (the mapping), which stays within ADR 0004's `heap.rs` rule; one more cache
file; `Vm::new` gains a `Snapshot` argument.
