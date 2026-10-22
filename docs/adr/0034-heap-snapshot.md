# ADR 0034 — Heap snapshot in a fixed region

Status: accepted · Date: 2026-10-22

## Context
Every run spends ~4.4 ms of hello's 8.3 ms in-process time constructing the same 91 runtime
modules in `scala.scalanative.runtime.package$.init` (research 32 §1). Their state is ~70 KB.
Identity hashes are object addresses (`System.identityHashCode`, `Class.hashCode`), so a
snapshot whose objects move corrupts every identity-keyed table built during `init`.

## Decision
1. **Verbatim region, never relocated.** The VM maps a 64 MiB region at a fixed hint address
   (macOS `0x2000_0000_0000`, else `0x5000_0000_0000`) before the collector starts. Until
   `init` returns, the VM trains: permanent allocations, objects and arrays all go to the
   region. The snapshot is the region's used prefix as it is; a restoring VM maps the same
   address and reads it in. If the hint is not granted the run starts cold, with the permanent
   heap in chunks as before.
2. **One root range.** Everything in the region is permanent and scanned conservatively by
   Immix, registered in 1 MiB steps as it fills (ADR 0004); no collector state is stored. After
   training the permanent heap keeps filling the region, then falls back to chunks.
3. **Training set.** What `init` constructs. On restore `init` is not run again: its effects
   that depend on the process (the arguments array, `ExecInfo.filename` and `startTime`) are
   ported to Rust (`snapshot/start.rs`, for Scala Native 0.5.12; a test fails on another
   release), and a snapshot without `ExecInfo` is refused. A deny list of environment-reading modules and a classification of every pointer-typed word
   (inside the region, C trampoline or runtime symbol by name, function reference by name,
   small integer) refuse any other snapshot. The walk follows what the VM's tables reach;
   training garbage, such as boxed pointers into ended frame stacks, is copied but not walked.
4. **Cache file** `snap-<hash of image key, BUILD, os, arch>.bin` beside the library image
   (ADR 0005), written by the first cold run through a staging file, touched on use and
   pruned like images; `SCIVOLO_NO_SNAPSHOT` runs cold without writing. A user `--cp` entry
   with a file in a package of the snapshot's classes starts cold.
5. **Tables by name.** The VM's modules, classes (in id order), literals, link-time
   properties, callbacks and counters are stored by name and re-interned; decoded definitions
   and lowered code are not stored.

## Consequences
- A new `unsafe` mapping in `heap.rs`; the region logic in `heap/region.rs` is safe code.
- Objects allocated during `init` are never collected, and weak references among them never
  cleared (none exist after `init`).
- One more cache file. `Vm::with_snapshot` takes `Snapshot::Cold`, `Restore(file contents)` or
  `Write(path)`; the hidden `SCIVOLO_SNAPSHOT=FILE` uses FILE instead of the cache's. A file
  of another program version or with a wrong body hash starts cold, and so does a restore
  that fails after the copy: the region is zeroed and the tables reset first.
- Field types are stored as a tagged binary encoding of NIR types with globals by name;
  trampolines are not stored but assigned again when a function value needs one.
- 2026-10-22, M4 under load: restoring with `init` run again saved 2 ms of hello and 3.8 ms
  of moderate (wall minima); without the re-run `init` takes 8 µs instead of 4.4 ms in
  process.
- The REPL starts from the same cache file (empty input: 14.6 → 8.8 ms wall minimum). Classes a
  script compiles into a package of the snapshot are not checked for shadowing.
- Rejected: a position-independent dump with relocations and an identity-hash side table
  (kept in reserve if a hint is ever refused in practice).
