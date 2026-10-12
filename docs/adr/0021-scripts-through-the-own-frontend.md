# ADR 0021 — Scripts go through the own frontend, scala-cli as announced fallback

Status: accepted · Date: 2026-10-12

## Context
`scalai FILE` compiled every script through scala-cli, starting a JVM on each cache miss. The
own frontend (`lower::compile`) covers a growing subset; ROADMAP phase 3 requires that what it
does not support falls back to the phase 1 path, never silently (research 09 §5, research 15
§2.2).

## Decision
- On a cache miss the own frontend compiles the script first; anything it refuses, including
  errors and panics, goes to scala-cli with a notice on stderr, which a cached result repeats.
  scala-cli then reports real errors. The user-facing rules are in the README.
- What the own frontend cannot honour falls back before it runs: `.sc` scripts (no wrapper
  yet) and `//> using` directives other than the Scala version it compiles.
- `SCALAI_FRONTEND` forces one path: `strict` for testing the own frontend, `jvm` for the old
  behaviour.
- The cache key holds the mode and a hash of the workspace's sources (`crates/scalai/build.rs`),
  so neither another mode nor an older build serves an entry.

## Consequences
- Scripts the own frontend covers run with no JVM, also on their first run.
- Every rebuild of `scalai` invalidates all compiled scripts.
- The own frontend's gaps reach users as the notice; its wrong code (accepted but miscompiled)
  reaches them without one; `just check` and the lowered programs in strict mode guard this.
