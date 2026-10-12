# ADR 0019 — Scripts go through the own frontend, scala-cli as announced fallback

Status: accepted · Date: 2026-10-12

## Context
`scalai FILE` compiled every script through scala-cli (JVM, 0.3–2 s on a cache miss). The own
frontend (`lower::compile`) covers a growing subset; ROADMAP phase 3 requires that what it does
not support falls back to the phase 1 path, never silently (research 09 §5, research 15 §2.2).

## Decision
- On a cache miss a `.scala` file goes through `lower::compile` over the library image's TASTy
  (`crates/scalai/src/frontend.rs`); a panic in it counts as a refusal.
- A refusal (unsupported construct, type or syntax errors, `.sc` files, a `//> using` directive
  other than `scala 3.9` / `scala 3.9.0`) prints `scalai: FILE: falling back to scala-cli
  (<reason>)` and compiles through scala-cli; scala-cli then reports real errors.
- `SCALAI_FRONTEND=strict` makes a refusal an error, `=jvm` uses scala-cli only; any other
  non-empty value is an error. The mode is part of the cache key, so a forced mode never reuses
  another mode's entry.
- The script is compiled on the interpreter's 1 GiB stack thread; `SCALAI_TIMEOUT` starts after
  compiling.

## Consequences
- A miss costs the frontend's time instead of a JVM start: 15 ms for hello end to end.
- A fallback entry stays cached under the default mode's key until the script changes, also
  after the frontend learns to compile it.
- `.sc` scripts need an own wrapper (the REPL's `wrap.rs` has the pieces) before they leave
  scala-cli.
