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
3. **Training set.** What `init` constructs; `init` is re-run on restore with the real `argv`.
   A deny list of environment-reading modules and a classification of every pointer-typed word
   (inside the region, C trampoline or runtime symbol by name, function reference by name,
   small integer) refuse any other snapshot. The walk follows what the VM's tables reach;
   training garbage, such as boxed pointers into ended frame stacks, is copied but not walked.
4. **Cache file** `snap-<image key>-<BUILD>-<os>-<arch>.bin` beside the library image (ADR
   0005), written by the first cold run, pruned with the image; `SCIVOLO_NO_SNAPSHOT` runs cold.
5. **Tables by name.** The VM's modules, classes (in id order), literals, link-time
   properties, callbacks and counters are stored by name and re-interned; decoded definitions
   and lowered code are not stored.

## Consequences
- A new `unsafe` mapping in `heap.rs`; the region logic in `heap/region.rs` is safe code.
- Objects allocated during `init` are never collected, and weak references among them never
  cleared (none exist after `init`).
- One more cache file. `Vm::with_snapshot` takes `Snapshot::Cold`, `Restore(file contents)` or
  `Write(path)`; the hidden `SCIVOLO_SNAPSHOT=FILE` restores FILE, or writes it after `init`
  when missing. A file of another program version or with a wrong body hash starts cold.
- Field types are stored as a tagged binary encoding of NIR types with globals by name;
  trampolines are not stored but assigned again when a function value needs one.
- 2026-10-22, M4 under load (minima): hello 10.8 → 8.8 ms, moderate 21.7 → 17.9 ms wall. The
  re-run `init` still decodes and lowers its own code (~1.5 ms of hello).
- Rejected: a position-independent dump with relocations and an identity-hash side table
  (kept in reserve if a hint is ever refused in practice).
