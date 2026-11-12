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

## Consequences
- A run types the unit once more per round of refusals: twice where no file needs a refused
  one. A file typed after a refused one may be refused for its error trees and left out with it.
- Until fresh names are counted per file (#115), the anonymous class and function names of an
  accepted file change when a file before it is left out.
