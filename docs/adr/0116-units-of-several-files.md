# ADR 0116 — A unit of several files keeps each file's settings and leaves out refused files

Status: accepted · Date: 2026-11-11

## Context
A cyclic codebase, as dotty's own sources (#241), is typed in one run. Its files differ in
their language imports, and a run of hundreds of files meets constructs the frontend refuses.
A unit was typed under one `Config`, refused when two files' settings differed, and refused as
a whole at its first refusal. dotty keeps the source version and the unit-wide features per
`CompilationUnit`, and reports the errors of each file.

## Decision
- Each `SourceUnit` carries the `Config` its parser ended with. The typer enters, completes and
  types each definition under the features and source version of its file
  (`Typer::enter_file`); the other settings are options, the same for all files. Source
  version `2.13`, which the lowering and the TASTy attributes read per unit, is refused in
  some files of a unit only.
- `compile_isolated` refuses a file, not the run: a refusal that lies in some files blanks
  them (spaces, line breaks kept, so the others keep their offsets) and compiles the rest again
  in a fresh universe. A file needing a refused one is refused in the next attempt, with its
  own message. A refusal in no file, or in files already left out, refuses the run.
- `scivolo-jvm` and `scivolo-lower` with several files write the output of the accepted files,
  report each refused file and exit with 1. `scivolo-aot` keeps refusing the whole program.

## Consequences
- A run with `n` refused files types the unit up to `n + 1` times.
