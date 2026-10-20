# Scripts

How `scivolo FILE` compiles and caches a script; the short form is in the README.

## Running

- `scivolo FILE.sc` or `scivolo [--main CLASS] [-O OPT]... FILE.scala`, also as a
  `#!/usr/bin/env scivolo` shebang.
- `-O`/`--scala-opt` passes a Scala compiler option, as `//> using options` in the script does.
- `--dep COORDINATE` adds a library and its transitive dependencies through Coursier's cache
  (`coursier` or `cs` on PATH, or `SCIVOLO_COURSIER`):
  - `group::artifact::version` and `group::artifact:version` select the Scala Native 0.5 /
    Scala 3 artifact;
  - Maven coordinates such as `io.circe:circe-parser_native0.5_3:0.14.10` are taken as named;
  - the interpreter's Scala standard library and Scala Native runtime versions stay;
  - in the REPL, dependencies remain after `:reset`.

## Frontends

- The first run compiles a `.scala` file through the own frontend (parser, typer, lowering) and
  prints its warnings on stderr.
- Compiler options it takes:
  - `-source` (no migration versions), `-no-indent`, `-indent`, `-old-syntax`, `-new-syntax`,
    `-preview`;
  - `-language:` with the features it implements, or `implicitConversions`,
    `reflectiveCalls`, `higherKinds`, `existentials`;
  - `-Xkind-projector`, `-nowarn`, `-Wnonunit-statement`.
- It compiles through scala-cli (`SCIVOLO_SCALA_CLI` names another binary) instead:
  - where the own frontend refuses the script;
  - for `.sc` files;
  - for `//> using` directives before the code other than `scala 3.9.0` and options;
  - for any other compiler option (also `-Werror`).
- scala-cli errors fail the run. stderr shows `scivolo: FILE: falling back to scala-cli (reason)`;
  later runs of that result show `scivolo: FILE: compiled by scala-cli (reason)`.
- `SCIVOLO_FRONTEND=strict` makes a refusal an error, printed with the syntax or type errors;
  `SCIVOLO_FRONTEND=jvm` always uses scala-cli.

## Cache

- The NIR goes to `SCIVOLO_CACHE`/`scripts`, keyed by the file's path and content, the `-O`
  options, the frontend mode and the `scivolo` build; later runs compile nothing.
- Files pulled in with `//> using file` and changed dependencies do not invalidate it.
- Compiled scripts and library images unused for 30 days are removed when the next one is built,
  compiled scripts at most hourly.

## Diagnostics

- Exceptions carry stack traces with file and line from the interpreter's frames.
- `SCIVOLO_TRACE=1` traces exceptions, `SCIVOLO_PROFILE=1` counts calls, `SCIVOLO_TIMEOUT=N`
  stops a run after N seconds, compiling the script included.
