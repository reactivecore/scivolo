# ADR 0116 — A unit of several files keeps each file's settings and finds all refused files

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
  (`Typer::enter_file`); the other settings are options, the same for all files. The lowering
  reads source version `2.13` per file, through the top-level class of a definition; an empty
  file, as a left-out one, has no source version.
- `compile_isolated` refuses a file, not the run: the refusals of an attempt that lie in some
  files (each file's first refusal while typing, its syntax errors, or the one refusal of a
  later stage) blank them (spaces, line breaks kept, so the others keep their offsets) and
  compile the rest again in a fresh universe. A file needing a refused one is refused in the
  next attempt, with its own message. An attempt whose refusals lie in no file, or in files
  already left out, refuses the run.
- `scivolo-jvm` and `scivolo-lower` with several files report each refused file, write nothing
  and exit with 1, as dotty: a file compiled without a left-out one may pick another `given`,
  overload or name (#251). With `--leave-out-refused`, used only by the measurement recipes
  (`lib-compile`, `bench-compile`; `lib-jar` shares lib-compile's command but takes nothing of a
  refused unit), they write the output of the accepted files and exit with 1.
  `scivolo-aot` keeps refusing the whole program at its first refusal.
- Each stage reports every file refused in it before the unit is typed again, as dotty's
  `Run.compileUnits` reports the errors of all units of a phase: typing gives each file's first
  unsupported construct and all type errors; the stages after typing run on the files without
  refusals or type errors. The lowering stops at its first refusal, as its phases walk the whole
  unit and share state across files. `Isolated::typings` counts the typings.
- **Not taken:** leaving refused files out after typing, without typing again (!818), which lost
  the super accessors and mixin members a refused file's lowering adds for classes of other files;
  predicting the files that need a refused one, which would refuse files that type against another
  definition once it is left out (the library jar's, with `--scalalib`).

## Consequences
- A unit takes one typing more than the depth of its chains of refusals; each lowering refusal
  costs a round. A file typed after a refused one may be refused for its error trees and left out
  with it.
- The checks after typing run, unlike dotty's, beside type errors and may report errors dotty does
  not; a unit with type errors is refused either way, so only the messages differ.
- Until fresh names are counted per file (#115), the anonymous class and function names of an
  accepted file change when a file before it is left out.
