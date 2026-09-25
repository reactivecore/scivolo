# ADR 0002 — The VM trusts NIR's memory accesses; C callbacks use a per-thread table

Status: accepted · Date: 2026-09-25

## Context
The javalib and nativelib NIR the VM runs uses pointer arithmetic, raw loads and stores,
`stackalloc` and C functions (ADR 0001). Objects must keep Scala Native's layout in real
memory, so the VM cannot check every address against an allocation it knows. C can also
call interpreted functions through plain function pointers, which carry no context.

## Decision
- Every access by address goes through `heap::read` and `heap::write`. Both are safe `fn`s
  that trust the running program, the way a CPU trusts machine code, and panic on the null
  page. Their `unsafe` blocks, plus the ones at the C boundary (`natives/ffi.rs`,
  `natives/callback.rs`) and in `stack.rs`, are the only ones in the VM.
- A C-callable trampoline finds its VM and function in a per-thread table of 64 slots. That
  table is the only VM state outside `Vm`. A VM fills its slots while `run_main` runs and
  clears them before it returns.

## Consequences
- A buggy NIR program, or a VM bug that computes a wrong address, is undefined behavior.
  Rust's type system cannot rule that out; the null-page check and tests are the guard.
- `unsafe` code stays in a handful of files that can be audited, instead of being spread
  through the interpreter.
- A C library that keeps a callback past `run_main`, or calls it from another thread, aborts
  with a message instead of reaching a dead VM.
