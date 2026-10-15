# ADR 0028 — Native executables via Cranelift, exceptions as flags

Status: accepted · Date: 2026-10-15

## Context
Scripts should also build as native executables (#113). Scala Native's own toolchain
(Reach, Interflow, LLVM, clang) takes 3.4–5.6 s for hello world. Research 22 compares
Cranelift, LLVM and smaller backends and measures a spike (`spikes/aot-cranelift`).

## Decision
- **Backend:** Cranelift (`cranelift-codegen`, `-frontend`, `-module`, `-object`) in the crate
  `crates/aot`, binary `scalai-aot`; NIR is translated to CLIF directly, one object file per
  build, linked with `cc` against `libsnrt.a`, the static archive of `native/`.
- **Exceptions:** a global flag slot checked after every call, jumping to the handler or
  returning to the caller, as the VM's `Result` unwinding does. No Itanium tables: Cranelift
  writes none on Mach-O, and `try_call` leaves unwinding to the embedder.
- **GC:** Immix scans stacks conservatively; no stack maps.

## Consequences
- Codegen takes microseconds per function; the external link dominates the build time.
- Every call costs a load and a branch for the flag; zero-cost exceptions would need our own
  `.eh_frame`/LSDA writer.
- No variadic calls or struct values in Cranelift's calls: C shims cover them.
