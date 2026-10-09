# ADR 0017 — REPL commands as the oracle's `ReplDriver` runs them

Status: accepted · Date: 2026-10-09

## Context
dotty's REPL commands are what `ReplDriver` does for each `ParseResult` command; the transcripts
are checked against a `ReplDriver` built by `scripts/repl-oracle`. Some commands need a JVM
(`:jar`, `:dep`, `:doc`, `:settings`), and dotty's `:load` ends the REPL on an unreadable path.

## Decision
- The command table is dotty's, in its order, so a prefix selects or is ambiguous exactly as
  there (`:s`, `:r`). `:kind`, `:require` and `:sh` print dotty's fixed texts; `:jar`, `:dep`,
  `:doc` and `:settings` print `-- unsupported: <command>`. `:help` lists only the commands
  that run.
- `:load` runs the whole file as one complete input, as `ReplDriver.run`. Its argument keeps
  trailing whitespace, as dotty's `(.*)`. A missing file prints Java's `getCanonicalPath`. A
  path that exists but cannot be read (a directory) prints `Couldn't read file "<path>": <error>`
  where dotty dies with `FileNotFoundException`; bytes that are not UTF-8 read as U+FFFD.

## Consequences
- Transcripts with relative `:load` paths need the repository root as working directory, which
  the scripted test sets.
