# ADR 0127 — Browser target

Status: accepted · Date: 2026-11-19

## Context
The REPL is to run in a web page (#283). Its frontend is portable Rust, but the VM leans on the
operating system in four places: Scala Native's C runtime loaded with `dlopen` (its externs,
and Immix, which scans the native stack conservatively), `mmap`, green threads on
`corosensei`, and a 1 GB native stack. A browser offers none of them: no C library to load, no
fixed addresses, no stack switching, about 1 MB of engine stack, and stable wasm32 aborts on
panic. The REPL transcripts and a probe of common inputs resolve 45 C symbols: the collector's,
memory and string functions (`memcpy`, `memmove`, `memcmp`, `memmem`, `strlen`), atomics,
pthread mutex and condition initializers, clocks, `write`, `environ` and platform probes
(`scalanative_set_os_props`, `scalanative_std*_fileno`).

Alternatives: compiling the C runtime with wasi-libc or emscripten (a new external toolchain
and libc, and Immix's stack scan is unsound where wasm locals are not in linear memory);
wasm-bindgen and xterm.js (new external crates and JS libraries for what a few hand-written
exports and a `<textarea>` do).

## Decision
- Target `wasm32-unknown-unknown`; the VM starts cold, without a snapshot or the C runtime.
- A cfg `portable`, set by the VM's build script on wasm32 or by its cargo feature `portable`,
  selects a second backing for the four OS dependencies, so it runs natively too and CI tests it:
  - the C externs the runtime needs are Rust functions in a name table
    (`natives/ffi/table.rs`); any other extern is refused as a missing native;
  - the heap bumps objects from allocator chunks, never frees them, and throws
    `OutOfMemoryError` past a cap (`heap/portable.rs`);
  - frame stacks come from the allocator; the main thread runs inline on the caller's stack;
    starting a thread is refused, and a wait without deadline is refused as a deadlock;
  - the stack guard allows a named budget below where the main thread starts.
- Program output to stdout and stderr goes to a sink in the VM, which the REPL returns before
  its result lines; clocks read `clock_gettime` natively and the host's `now_ms` on wasm32.
- The library image is read from bytes the page downloads (`Image::from_bytes`).
- `crates/web`, the module the page loads, may hold `unsafe` for its exported ABI and one
  global session, the only state outside `Vm` there.

## Consequences
- Native default builds keep the C runtime, Immix and green threads unchanged.
- Without a collector a session's garbage accumulates until the cap; a collection between
  inputs is a later step. Immix's behaviour (collection while running) is case 1 of the
  conformance rules only as far as no live object is freed; programs that rely on collection
  to stay under the cap end with `OutOfMemoryError` where Immix would go on.
- Threads, files, sockets, processes, `:load` and C libraries are refused in the browser.
- A panic aborts the wasm instance; the page restarts it, losing the session.
