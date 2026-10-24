# 36 — Compiling the Scala Native standard library with the own frontend (#168)

Date: 2026-10-24. Claims are [V] verified by reading or running, [I] inferred. Paths: `SN/` =
`ref/scala-native/` (v0.5.12), `P/` = `SN/nscplugin/src/main/scala-3/scala/scalanative/nscplugin/`,
`Y/` = `crates/typer/src/`, `L/` = `crates/lower/src/`. Measured on main 409ba6be with
`target/release/scivolo-lower` over every `.scala` file of a module, one file per run, against the
published jars in `ref/libs` (`tmp/r36/measure.sh`; results `tmp/r36/<module>.tsv`); a second
run used a *measurement variant* of the binary with two local changes that were not committed:
the Scala Native annotations no longer refused (`Y/namer/annotations.rs:16-40`) and a unit's
package members replacing the classpath's of the same name (9 lines in `Y/namer/mod.rs:120`).

**Recommendation.** The C parts need no decision beyond what exists: they are resources of the
published jars, compiled by clang at link time by Scala Native and by `crates/snrt` for us; keep
that (ADR 0042, §2). "The same artifacts" means byte-identical NIR per file plus the copied C
resources; `.class` files are out of scope and TASTy is a later stage (ADR 0042, §4). The
frontend already lowers 200 of javalib's 602 files and 64 of nativelib's 107 with the variant,
228 of its 311 javalib NIR files identical to the published ones [V]. The first step is a `just
lib-compile MODULE` recipe (§5.1) and the two variant changes made real (§3 G1, G2); then extern
definitions (G3), which gate all of clib and posixlib.

## 1. What the library is

### 1.1 Modules and sources [V `SN/project/Build.scala`, `MultiScalaProject.scala:269-279`]

| module | depends on | `.scala` (main / scala-3 / 2.13+) | C resources | published jar `…_native0.5_3-0.5.12` holds |
|---|---|---|---:|---|
| nativelib | javalib-intf (Provided, 2 Java files) | 98 / 5 / 0 (+2 scala-2, +2 scala-next) | 173 (76 c, 5 cpp, 5 S, 87 h) | 721 nir, 184 tasty, 619 class, C |
| clib | nativelib | 20 / 1 / 0 (+1 scala-2) | 13 c (+1 `.gyb`) | 115 nir, 24 tasty, 108 class, C |
| posixlib | nativelib, clib | 67 | 45 c, 2 h | 270 nir, 94 tasty, 270 class, C |
| windowslib | nativelib, clib | 34 | 16 c | 130 nir, 49 tasty, 129 class, C |
| javalib | posixlib, windowslib, clib | 599 / 2 / 0 (+1 scala-2) | 12 c, 1 h | 3,503 nir only (`NIROnlySettings`) |
| auxlib | nativelib, clib | 14 / 1 | — | 86 nir only |
| scalalib (3.9.0+0.5.12) | auxlib | fetched `scala-library` 3.9.0 sources + 105 `.patch`, 2 `.scala` overrides | — | 4,982 nir only |

- Source directories for a Scala 3 build: `src/main/scala`, `scala-3`, `scala-2.13+`
  (`MultiScalaProject.scala:275`); `scala-2` and `scala-next` are not compiled for 3.9.
- Every module compiles with nscplugin (`.withNativeCompilerPlugin`), javalib and
  javalib-ext-dummies with `-P:scalanative:genStaticForwardersForNonTopLevelObjects`
  (`Settings.scala:706-713`); javalib is compiled against the build JDK, so its sources use
  JDK-only types (`@FunctionalInterface`, `java.time.Instant`, `javax.*`, `FileStore`) that no
  published artifact defines and the linker never reaches [V §3].
- scalalib for 3.8+ is "self-contained" (`Settings.scala:740-745`): the 3.9.0 `scala-library`
  sources are fetched, overridden by `scalalib/overrides-3*/` (105 patches applied with `git
  apply`, `:855-960`), compiled with `-Yexplicit-nulls -language:implicitConversions
  -Wconf:any:silent` (`Build.scala:515-528`), and 42 specialized classes (`Function0..2`,
  `Tuple1/2`, `Range`, steppers, `MurmurHash3`, …) are copied from the Scala 2.13 scalalib jar
  because Scala 3 emits no specializations (`:529-617`).
- NIR-only jars drop `.class` and `.tasty` at packaging (`Settings.scala:694-705`); the other jars
  keep both. Positions in NIR are relative to each source directory
  (`java/lang/VirtualThread.scala:199:88` for a `scala-3/` file, `P/NirPositions.scala:49-66`:
  `positionRelativizationPaths`, else `-sourcepath`/`-sourceroot`) [V dump].
- The C sources live in `src/main/resources/scala-native/` and are packaged as resources with a
  `scala-native.properties` descriptor (`compile.c.options = -std=gnu11`, include paths, GC
  project flag) [V jar listing]. Nothing in the sbt build compiles them: Scala Native's toolchain
  unpacks and compiles them at link time with clang/clang++ (`SN/tools/.../NativeLib.scala:39-142`,
  `LLVM.scala:49-115`), filtered by GC, `@link` names and `SCALANATIVE_*` defines.

### 1.2 nscplugin's three phases [V `P/ScalaNativePlugin.scala:56`, `P/GenNIR.scala:15-16`]

1. `PrepNativeInterop` (297 lines, after typer): marks top-level `= extern` defs/vals `@extern`,
   extension methods and implicit-class constructors of extern objects `@nonExtern`, rewrites
   exported varargs proxies, and gives `Enumeration` values their names (`:59-144`).
2. `PostInlineNativeInterop` (149 lines, after inlining): attaches the unerased types of
   `CFuncPtr.fromScalaFunction`, `CFuncPtr.apply`, `stackalloc[T]` and extern varargs, and
   rewrites `sizeOf[T]`/`alignmentOf[T]` to `Intrinsics.sizeOf(classOf[T])` (`:49-147`).
3. `GenNIR` after `MoveStatics` (`NirCodeGen`, `NirGenStat` 970, `NirGenExpr` 3,189, `NirGenType`
   372, `NirGenName` 187, `NirPrimitives` 234, `GenReflectiveInstantisation` 402,
   `GenNativeExports` 186, `AdaptLazyVals` 248 lines).

### 1.3 How scivolo consumes it today [V]

- `crates/libs/src/lib.rs:16`: six jars (auxlib, javalib, nativelib, clib, posixlib, scalalib) from
  `$SCIVOLO_LIBS` or the Coursier cache, plus `scala-library-3.9.0.jar` for TASTy and the
  checked-in `libs/javalib-tasty-0.5.12.jar` (sbt-built by `just javalib-tasty`,
  `scripts/javalib-tasty.sh`), all in one library image (ADR 0005). windowslib is not loaded.
- The typer's universe reads TASTy from `scala-library`, nativelib, clib, posixlib and the javalib
  TASTy jar (`Y/dump.rs:19-25`); the names javalib gives JDK classes (`_String`, `_Object`, …) are
  mapped in `L/codegen/names.rs:14-24` and `crates/tasty/src/symbols/jdk.rs`.
- The overlay (ADR 0009) adds and replaces javalib methods from `overlay/src`, compiled by
  scala-cli + Scala Native into `overlay/nir` (`scripts/overlay.py`).
- The C runtime: `native/` holds 151 of the jars' 262 C/C++/S/header files (clib, posixlib and
  javalib complete; nativelib's GC immix + shared, `platform/`, and 7 top-level files; not
  libunwind, `eh.cpp`, `delimcc`, `zone`, commix/boehm/none GCs, windows) plus two patched
  files (`gc/immix/Marker.c`, `gc/immix_commix/headers/ObjectHeader.h`) and `native/scivolo/`
  glue (`gc_glue.c`, `libc_glue.c`). `crates/snrt/build.rs` compiles every `.c` with `$CC`
  (default `clang`, `-std=gnu11 -O2 -fPIC -DSCALANATIVE_GC_IMMIX …`) into `libsnrt.{dylib,so}`
  for the VM (`dlopen`, `crates/vm/src/natives/ffi/dl.rs:12-18`) and `libsnrt.a` for native
  executables (`crates/aot/src/link.rs:46`).

## 2. The C parts

1. Scala Native itself never compiles the C sources in its build: they travel as jar resources and
   clang compiles them when a program is linked (§1.1). "Create the same artifacts" for C therefore
   means copying `src/main/resources/scala-native/**` into the jar unchanged, `.gyb`, `.properties`
   and `.modulemap` included [V].
2. scivolo already uses clang for them (`crates/snrt/build.rs:56-70`), once per build of the Rust
   workspace, not per program; the VM needs no C compiler at run time and native executables only
   a linker (`cc`). The `cc` crate would add nothing but a dependency: the build already parallelizes
   and honours `CC`/`AR` [V].
3. Not compiled on purpose: `eh.cpp`/libunwind (the VM unwinds its own frames, ADR 0001),
   `delimcc` (continuations, `runtime/Continuations.scala`, Scala 3 only, unused by the VM), `zone`
   (`SafeZone` is `scala-next` only), the other GCs (ADR 0004) and windows (ROADMAP "Platforms").
   A program linking them through `@link` would fail at `dlsym`; none of the suites does [I].
4. Recommendation (ADR 0042): keep `native/` as the curated copy, add a `just native-stale` check
   that lists files differing from the jars' resources except the two patched and the glue files
   (~40 lines of Python beside `scripts/overlay.py`), and when scivolo compiles a module, write the
   module's resource directory into its jar verbatim. No clang in scivolo's binaries.

## 3. What the frontend does on these sources today

Per file, one process each (≈0.4 s including process start [I wall clock]); `ok` = NIR written.

| module | files | ok (main) | ok (variant) | first refusal, main | first refusal, variant |
|---|---:|---:|---:|---|---|
| clib | 20 | 0 | 13 | 17 `@extern` refused, 3 overloaded reference | 3 native interop (lowering), 4 typer singles |
| posixlib | 67 | 0 | 34 | 63 `@extern`, 2 `@link`, 2 overloaded reference | 16 assignment to a non-variable, 13 native interop, 4 implicit search |
| nativelib | 107 | 24 | 64 | 61 overloaded reference, 9 `@extern`, 3 `@alwaysinline` | 10 `= _`, 5 native interop, 3 erased branches, 3 setters, 3 universal equality, singles |
| javalib | 602 | 156 | 200 | 272 overloaded reference, 43 `FunctionalInterface`, 34 member not found | 180 object of a Java class, 46 `FunctionalInterface`, 35 member not found, 24 adaptation, 18 `= _` |
| auxlib | 15 | 5 | 5 | 6 object of a Java class, singles | same |

NIR of the `ok` files against the jars (`scivolo-nir-dump --debug --sorted`, source directories
stripped from positions; `tmp/r36/cmp.sh`) [V]: javalib main 146 same / 41 differ of 187 files;
variant javalib 228 / 83 of 311, nativelib 108 / 189 of 297 (+3 `scala-next` files the jar lacks),
posixlib 5 / 67, clib 0 / 26 (the variant drops the annotations' attributes, so `extern` and
`alwaysinline` are missing: expected), auxlib 6 / 0.

Groups, by what they gate [V sites, I lines]:

| id | group | files gated | site | dotty / nscplugin | lines |
|---|---|---:|---|---|---:|
| G1 | a unit's package member must replace the classpath symbol of the same name ("overloaded reference" at the definition, probe `tmp/r36/probe/string.scala`) | 338 | `Y/namer/mod.rs:120` enters beside the TASTy symbol; `Y/typer/package_members.rs:24` returns both | `Namer.scala:152-162` (`isDefinedInCurrentRun`), `:372` `unlinkFromFile` | 15 |
| G2 | Scala Native annotations: accept, and emit their attributes | 90 + every later file | `Y/namer/annotations.rs:16-40` refuses 18 of them; `L/codegen/body.rs:43` knows only `@inline`/`@noinline`, `class.rs:22` only `Abstract` | `NirGenStat.scala:70-90` (class), `:131-170` (`@align`), `:187-196` (fields), `:327-370` (methods: `alwaysinline`, `noinline`, `nooptimize`, `nospecialize`, `stub`, `link`, `define`, `linkCppRuntime`, `blocking`, `safePublish`) | 60 |
| G3 | `@extern` objects, methods, vals and their calls | clib 17, posixlib 63, nativelib 9, javalib's natives | `L/codegen/class.rs:17`, `body.rs:53`, `callee.rs:31` refuse "native interop"; `is_extern` exists (`L/symbols.rs:53`, `crates/tasty/src/symbols/entries.rs:82`) | extern trait/module (`NirGenStat.scala:92-120`), `genMethod` extern branch `:276-300`, `genExternMethodSig`/`genExternType` (`NirGenType.scala`), accessors and args (`NirGenExpr.scala:1271-1310, 1734-1770, 2743-2800`), `PrepNativeInterop:59-144` | 250 |
| G4 | intrinsics: `runtime.Intrinsics` (raw loads/stores, `elemRawPtr`, casts, `stackalloc`, `classFieldRawPtr`, `sizeOf`, `alignmentOf`, unsigned ops), `CFuncPtr.apply`/`fromScalaFunction`, `c"…"`, `LinktimeInfo`/`@resolvedAtLinktime` (`Attr::LinktimeResolved`, `Inst::LinktimeIf`), `@struct` | nativelib `unsafe/*`, `runtime/*`; javalib's `stackalloc`/`CString` users | none; `crates/nir` has the ops (`nir.rs:215,257,272-284`), `L/codegen/fields.rs:80` has `classFieldRawPtr` | `NirPrimitives.scala`, `NirGenExpr.scala:1109-1156, 1158-1230, 2274-2400, 2575-2960`, `NirGenStat.scala:462-520` (`genStruct`), `PostInlineNativeInterop` | 600 |
| G5 | javalib *is* the JDK: unit classes in `java.*` are Scala classes, not "Java classes" (`object DefaultVirtualThreadScheduler` refused as "object of a Java class used as a value"; calls through static forwarders where the jar calls the module, `FileReader` diff); members a JDK interface redeclares (12); JDK-only names (`FunctionalInterface` 46, `SuppressWarnings`, `java.time`, `javax`, `Locale`, `FileStore`, `AsynchronousFileChannel`); windowslib TASTy missing (33 javalib files import `scala.scalanative.windows`) | 180 + 46 + 35 + 12 | `Y/typer/java_classes.rs:16-21` and `L/symbols.rs:31-40` decide by package name, must apply to TASTy symbols only (`Symbol.tree.is_none()`); `Y/dump.rs:19-25`, `crates/libs/src/lib.rs:16` lack windowslib | javalib compiles against the JDK | 60 + a stub TASTy jar `libs/jdk-stubs-*.jar` built like `javalib-tasty` from ~40 lines of Scala stubs (`scripts/jdk-stubs.sh`) |
| G6a | `var x: T = _` | 28 | `Y/typer/idents.rs:62` "unresolved name _" | `Typer.typedValDef`: `_` rhs of a `var` is the default value (`Trees.isWildcardArg`) | 15 |
| G6b | setters through an implicit conversion or symbolic getter: `ptr._1 = v` (`CStructOps`), `!p = v` (`Ptr.unary_!_=`), `in = null` (inherited param-accessor var of a classpath class) | 27 | `Y/typer/assign.rs:21` | `Typer.typedAssign`: setter on the converted qualifier's type, `lhsCore.qualifier` | 40 |
| G6c | wildcard-typed receivers `Consumer[_ >: T]`, `Collection[_ <: E]`: adaptation, "result type of a wildcard argument" | 32 | `Y/typer/adapt.rs:124`, `Y/typer/apply.rs:115` | `Typer.adapt` capture conversion (`captureWildcards`), `Inferencing` | 80 |
| G6d | singles: SAMs outside the subset (7), abstract member overriding a concrete one (5), inline-body selections (3), `NotGiven` beside top-level implicits (2), implicit search too costly (3), `scala-2` syntax (expected) | ≈ 25 | — | — | — |
| G7 | byte-level NIR differences of files that lower: parameter names of methods overriding classpath methods (`value` vs `x$0`, `RecursiveAction`), module calls vs static forwarders (G5), lexical-scope positions of `match` cases (`FileSystemException.scala:17-25`: the case body's column vs the pattern's) | 41 javalib files | `L/codegen` positions, `L/phases` bridges | `NirGenExpr` `genMatch` scopes | 60 |

The lowering refusals behind the typer's are few and known: `assignments through setters` (6
files, research 34 L5), `branches of arrays and other erased classes` (4), `universal equality of
primitive values` (3), `arrays of generic or several dimensions` (1) [V].

## 4. Verification: "the same artifacts"

1. **NIR, byte for byte.** The oracle is the published jar: `unzip` its `.nir` into
   `tmp/lib/jars/<module>/` once, then diff `scivolo-nir-dump --debug --sorted` of each lowered
   file against it as `just nir-cmp` does (`Justfile:157-174`). Positions need the file's path
   relative to its source directory (`scala/scalanative/libc/stdlib.scala`, §1.1):
   `scivolo-lower` names the source by its basename (`L/bin/scivolo-lower.rs:73-75`), so it gets a
   `--sourceroot DIR` option (positions relative to `DIR`, default the file's directory, ~10 lines).
   The `tests/programs` oracle stays as it is (basenames, as scala-cli compiles them).
2. **C resources**: `diff -r` of the jar's `scala-native/` against the module's
   `src/main/resources/scala-native/` — trivially equal when copied; `just native-stale` (§2.4)
   checks `native/` against them.
3. **TASTy**: not produced (no pickler in `crates/typer`; ADR 0011 writes NIR only) — stage 4
   (§5); until then a module's jar takes the upstream TASTy (or the sbt-built one, as
   `libs/javalib-tasty-0.5.12.jar`). `tasty::symbol_table` of ours vs theirs will be the
   comparison when it exists.
4. **`.class`**: never (no bytecode backend, ROADMAP Phase 5). The toolchain reads NIR only; the
   class files serve JVM-side tooling [V `Settings.scala:694-705` drops them where unneeded].
5. **Behaviour**: the test programs, the Scala Native unit tests (`just sn-tests`) and M4/M5 run
   with the self-compiled jars on the classpath (`SCIVOLO_LIBS` pointing at a directory of our
   jars) — the identical-NIR check makes this redundant where it passes and decisive where NIR
   differs legitimately (positions, local names).

## 5. Plan

Ranked by files unblocked per line, each an MR gated by `just gates`, `just lib-compile` for the
module(s) it targets with no file worsening, and `just snapshot-diff programs lower typer`.

| # | MR | lines | unblocks |
|---|---|---:|---|
| 1 | `just lib-compile` recipe + `--sourceroot` (§5.1) | 170 | the baseline of everything below |
| 2 | G1 unit members replace classpath symbols | 15 + test | 338 files reach their real refusal |
| 3 | G2 annotations → attributes | 60 | 90 files past the namer; `alwaysinline`/`extern` attrs in NIR |
| 4 | G6a `= _`, G6b setters | 55 | 55 files |
| 5 | G3 extern definitions and calls | 250 | all of clib and posixlib, nativelib's `libc`/`posix` users; windowslib |
| 6 | G5 javalib as the JDK (unit classes are Scala), windowslib TASTy, JDK stubs jar | 60 + 40 Scala + script | ≈ 270 javalib files, auxlib's 6 |
| 7 | G6c wildcard receivers | 80 | 32 files |
| 8 | G4 intrinsics, `@struct`, `CFuncPtr`, `c"…"`, linktime | 600 (3–4 MRs: raw pointer ops; stackalloc/sizeOf/CFuncPtr; linktime; struct) | nativelib `unsafe/*`, `runtime/*`; javalib `CString` users; identical NIR for extern files |
| 9 | G7 byte-level: override parameter names, match-case scope positions | 60 | 41 javalib files identical |
| 10 | stage 2: one module as one unit (`compile_sources`, `L/units.rs`) against its dependencies' jars only, jar assembly (NIR + resources + manifest), `--sourceroot` per source dir | 150 | the artifact; needs #115 (fresh names per file, `L/units.rs:26,42`) for 600-file units |
| 11 | stage 3: scalalib — fetch `scala-library` 3.9.0 sources, apply the 105 patches, `-Yexplicit-nulls` (the typer lacks explicit nulls: refuse or implement, ~300), copy the 42 specialized classes from the 2.13 jar | 200 + nulls | the scalalib jar |
| 12 | stage 4: TASTy pickler (`TreePickler` ≈ 1,000 lines of dotty + names, positions) | 1,200 | jars that replace upstream's for the frontend; `javalib-tasty` without sbt |

Order 1→2→3→4 fits one session and is disjoint by file (`scripts/`, `Y/namer/mod.rs`,
`Y/namer/annotations.rs` + `L/codegen/body.rs`, `Y/typer/idents.rs` + `assign.rs`). 5 and 8 share
`L/codegen/` with research 34's leftovers: one coder. 6 touches `crates/libs` (the VM's jar list)
and the typer's universe: check `just check` startup (`bench hello`) does not regress from the
seventh jar.

### 5.1 First step: `just lib-compile MODULE` (today)

`scripts/lib-compile.py` (~150 lines, beside `scripts/overlay.py`) and the recipe
`lib-compile module filter=""` (`cargo build -q --release -p scivolo-lower -p scivolo-nir-dump`
first):

1. Sources: `SN/<module>/src/main/{scala,scala-3,scala-2.13+}/**/*.scala` (MultiScalaProject's
   rule), `ref/scala-native` fetched by `just fetch-scala-native`; modules `nativelib clib
   posixlib windowslib javalib auxlib`.
2. Per file (stage 1): `scivolo-lower --sourceroot <dir> FILE tmp/lib/<module>/out/<rel>/`, 60 s
   alarm as `Justfile:36`; stdout lists the NIR names, stderr's first line is the refusal.
3. Compare each written `.nir` with the jar's (unpacked once into `tmp/lib/jars/<module>/`) as
   `nir-cmp` does: `same` / `differ N` (lines) / `missing`.
4. Outputs: `tmp/lib/<module>/results.tsv` (file, ok|refused|timeout, reason, nir same/differ/
   total), `refusals.txt` (reasons grouped as `Justfile:112` does, positions stripped),
   `cmp.tsv`, and one summary line `javalib: 200 ok, 402 refused; nir 228 same, 83 differ`. A
   `baseline` copy for `snapshot`-style regression checks (`scripts/snapshot.py` gains the
   `lib` oracle later, 30 lines).
5. `--sourceroot` in `L/bin/scivolo-lower.rs` (10 lines): the source name is the path relative
   to it. The main checkout keeps the numbers of §3 as the baseline to beat.

Then MR 2: in `Y/namer/mod.rs:120`, after `define`, for a package owner call `u.decl(owner, name)`
and `u.unenter` every result that is a TASTy symbol (`tree.is_some()`) of the same namespace
(`kind.is_term()`), as dotty's `Namer.scala:152-162`; a unit test in `crates/typer/tests` typing
`package scala.scalanative.libc; object string { def f = 1 }` and a `tests/programs`-style check
that a redefinition of a classpath class is not an error. The variant proved the effect: javalib
156 → 200 files, nativelib 24 → 64 [V].

## 6. Decisions and issues

- ADR 0042 (C parts stay resources compiled by clang in `crates/snrt`; what "the same artifacts"
  means; stage order NIR → jar → TASTy).
- #168 gets the checklist of §5 as issues per MR; #115 (fresh names per file) is a prerequisite of
  stage 2 and gets a note.
- Risk: javalib's "JDK view" (G5) is a heuristic by package name in two crates; the fix must not
  change how scripts see `java.*` (every `tests/programs` entry compiled from source, `just
  aot-src-all`, is the regression test).
