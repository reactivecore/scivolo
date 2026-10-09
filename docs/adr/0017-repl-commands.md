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
  path that exists but cannot be read (a directory, bytes that are not UTF-8) prints
  `Couldn't read file "<path>": <error>` where dotty dies with `FileNotFoundException` or
  `MalformedInputException`. A `:load` inside 16 others prints `Couldn't load file "<path>"`
  instead of running, where dotty dies with a stack overflow.
- `:imports` lists the top-level imports of the inputs whose module was constructed, printed
  from the typed tree as `RefinedPrinter.importText`. An identifier as path is its symbol's
  owners, or its name alone when the owner is `scala`, `java.lang` or `Predef`; a selector's
  bound is its source text. The `scala`
  binary runs `import dotty.vendored.pprint.pprintln` as a first input, which `:imports` lists
  and which numbers the first user input `rs$line$2`; the oracle, and so this REPL, has no such
  predef.

## Consequences
- An identifier naming an inherited member of an imported object shows its defining class as
  owner, where dotty shows the object it was imported from.
- A selector's bound prints as written, where dotty prints the typed bound (`Ordering[ Short ]`
  as `Ordering[Short]`, `?` as `? >: Nothing <: Any`); the typer refuses bounds for now.
- Transcripts with relative `:load` paths need the repository root as working directory, which
  the scripted test sets.
