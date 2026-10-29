# 41 — Scala Native's library through the own frontend after 2026-10-29 (#168)

Date: 2026-10-29. Claims are [V] verified by reading the code or running it, [I] inferred. Paths:
`Y/` = `crates/typer/src/typer/`, `Yn/` = `crates/typer/src/namer/`, `L/` = `crates/lower/src/`,
`SN/` = `ref/scala-native/`, `P/` = `SN/nscplugin/src/main/scala-3/scala/scalanative/nscplugin/`.
Measured on main e9e7ae3d with `just lib-oracle` (Scala 3.9.0, ADR 0046; fresh build, 4,816 oracle
NIR files) and `just lib-compile all` (`tmp/lib/<module>/results.tsv`, `cmp.tsv`, `diffs/`) [V]:

| module | files | ok | refused | NIR same | differ | missing | oracle-only | 10-27 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| nativelib | 103 | **91** | 12 | 438 | 62 | 0 | 219 | 88 |
| clib | 21 | **19** | 2 | 57 | 1 | 0 | 57 | 18 |
| posixlib | 67 | **65** | 2 | 246 | 2 | 0 | 22 | 59 |
| windowslib | 34 | **34** | 0 | 121 | 9 | 0 | 0 | 32 (10-26) |
| javalib | 601 | **499** | 102 | 1,697 | 228 | 0 | 1,571 | 488 |
| auxlib | 15 | **6** | 9 | 6 | 2 | 0 | 78 | 6 |
| all | 841 | **714** | 127 | 2,565 | 304 | 0 | 1,947 | 691 |

No timeout, no crash. Every oracle-only NIR file belongs to a refused source (its own name or a
class of another name defined there, `Throwables.scala`, `RefTypes.scala`): no file that lowers
lacks a class (`tmp/r41/oracle_only.py`) [V]. Method: `tmp/r41/refused_lines.py` prints the source
line of each refusal; `tmp/r41/diff_kinds.py` and `diff_classes.py` group the NIR diffs; probes in
`tmp/r41/probe/` reproduce the largest group with `scivolo-lower` [V].

**Recommendation.** 127 refusals in ~45 texts, but two causes hold 37 of them and a third 14:
JDK classes javalib's sources import that javalib does not define (25 files, Scala stubs only),
private members of a companion used from its class (12 [V], the typer keeps `Local`, +9 access
checks in the same file [I]), and private type aliases plus the extern-object members nscplugin
turns into ordinary methods (14). Of the 304 differing NIR files 111 differ in debug information
only (positions, local names, scopes). Four stacked MRs for one coder (§3): **1** JDK stubs and the
macro VM in `lib-compile` (30 files, ~90 lines), **2** `Y/access.rs` (21 files, ~70), **3**
positions (111 NIR files identical, ~60, probes first), **4** extern-object members, native
boxing, private type aliases (14 files, ~130). After them: auxlib's redefinitions of
scala-library's Java classes (8), the code-level NIR groups (§1.3), the 37 typer singles.

## 1. Attribution

### 1.1 Refusals (127) by cause

| cause | site | nat | clib | posix | java | aux | files |
|---|---|---:|---:|---:|---:|---:|---:|
| JDK classes absent from javalib: `java.time.{Duration, Instant}` (6 files), `java.lang.reflect.{AnnotatedElement, Field, Method}`, `java.security.{SecureRandom, PrivilegedAction, PrivilegedExceptionAction, CodeSigner}`, `java.io.Object{Input,Output}Stream`, `java.nio.channels.{DatagramChannel, AsynchronousFileChannel}`, `java.nio.file.{FileStore, WatchService}`, `java.util.Locale`, `java.net.URL`, `java.lang.Deprecated`, `javax.security.auth.Subject` | `libs/jdk-stubs/JdkStubs.scala` has `FunctionalInterface`, `SuppressWarnings` only [V] | 1 | | | 24 | | **25** |
| private `val` of a companion used from its class, as `Vector.DEFAULT_SIZE` or through `import ArrayDeque._`: "a selection of a value that is not a field" (6), "a reference to NCPU" (6) | `Y/access.rs:53` `unset_local` keeps `Local`; `L/phases/getters.rs:68` then makes the value a constructor local or field of the object, `L/codegen/expr.rs:95,126` find nothing [V probes `local.scala`, `ident.scala`] | 1 | | | 11 | | **12** |
| "inaccessible member": `private[java] def isCwd`, `protected[_String] var value`, `private[ref] def enqueue`, `private[nio] _offset`, parameter accessor `prefixLength: Integer` used as `prefixLength > 0` | `Y/access.rs:22,64` | 1 | | | 7 | 1 | **9** |
| private type aliases of an object in signatures (`GC.pthread_create(thread: Ptr[pthread_t])`, `private type pthread_t`), `zlib#z_streamp` projections, `private type alloc_func = zlib#alloc_func` | `Y/dependent_results.rs:78`, `Y/select.rs:173` | | | | 6 | | **6** |
| macro `UnsafePackageCompat.validateSize` behind `stackalloc[T](n: Int)`, `alloc[T](n: Int)`: `scivolo-lower` builds the macro VM only with `--cp` (`L/bin/scivolo-lower.rs:48`); with one, the 3.1.3 TASTy of nativelib refuses "a quote of Scala 3.0 or 3.1" [V] | `Y/inline_refs.rs:38` | | | | 5 | | **5** |
| members of extern objects nscplugin makes ordinary methods: runtime overloads `def malloc(size: Int) = malloc(size.toCSize)` (`P/NirGenStat.scala:655` `isExternMethodRuntimeOverload`), implicit classes (`P/PrepNativeInterop.scala:66-76` `NonExternClass`), nested objects (`GC.MutatorThreadState`) | `L/codegen/externs.rs:51,114,118` | 1 | 2 | | 1 | | **4** |
| native annotations and boxing: `@align("c")` with a group, `@exported` (`P/GenNativeExports.scala`, 186 lines), `RawPtr` boxed in `implicit class LockWord(val value: RawPtr)` | `L/codegen/annotations.rs:85,53`, `erased.rs:62` | 3 | | | 1 | | **4** |
| auxlib redefines scala-library's Java classes: `object BoxedUnit`, `Statics`, `BooleanRef` "overloaded reference" (the namer unenters TASTy symbols only), `BasicNode`/`MainNode` unresolved across files, `BoxedUnit.UNIT`, `scalanative.runtime.BoxedUnit` | `Yn/mod.rs:161` `tree.is_some()` | | | | | 8 | **8** |
| "member not found" on wildcard-bounded receivers: `keyExtractor(o1).compareTo`, `f.apply(x).toCompletableFuture()`, `t.doExec()`, `key.compareTo(e.asInstanceOf[T])`; `ProcessBuilder.setEnv` | `Y/` selection | | | | 5 | | **5** |
| lowering singles: "`this` of an enclosing class" in a by-name or `Zone` lambda (3, #165), `new Array(n)` typed by the expected `Array[Byte]` (2), `var last: E = _` of an outer type parameter in an inner class (2 [V probe `inner.scala`]), `lock`/`c` of an anonymous subclass (3), a primitive cast, equality of an opaque type | `L/codegen/modules.rs:66`, `array.rs:70`, `expr.rs:95`, `cast.rs:135`, `L/phases/elim_opaque.rs:22` | | | | 12 | | **12** |
| typer singles (§1.2) | | 5 | | 2 | 30 | | **37** |

### 1.2 The 37 typer singles

adaptation 5 (GenBuffer, Charset, Spliterators, ThreadPoolExecutor, StreamImpl); "function
parameter without a type" 3 (`memmem(_, _, _, _)` against `CFuncPtr4`, `(p, l) => new X(p, l)`
in UnixFileSystemProvider, WindowsFileSystemProvider); "class tag of an undetermined type" 2
(`Array.empty` as `String*`: Files, FileHelpers); "method value of a single abstract method type"
2 (ArraysJDK9Methods, HashSet); and 25 one-offs: ambiguous implicit conversions (VirtualThread),
implicit method result under type variables (File), local type escaping (PushbackReader), no
applicable alternative (ProcessBuilder), default argument of a local method
(WindowsProcessFactory), refinement referring to the refined type (Buffer), block type with local
symbols (PosixFileAttributeViewImpl), constructor proxy (PosixUserPrincipalLookupService), type
tree (AbstractList), implicit candidate being completed (Arrays), bounds referring to type
parameters (Collections), type shadowing an inherited class (LinkedHashMap), unresolved `V`
(ScheduledThreadPoolExecutor), cyclic reference (ForkJoinPool), ambiguous overload
(RandomGeneratorFactory), `new A` of an alias (Collectors), lifted curried call
(WindowsDosFileAttributeView), `unapplySeq` beside a view (WindowsPath); nativelib: `UnsupportedFeature`
(Continuations), skolem (LazyVals), inline accessor (UnsafePackageCompat), `ConcurrentHashMap[Long, _]`
bounds (NativeThread), default getter (Throwable); posixlib: implicit search too deep (signal),
ambiguous reference `c._7._1` (stat).

### 1.3 NIR differences (304 files)

| kind | java | nat | win | posix | clib | aux | files |
|---|---:|---:|---:|---:|---:|---:|---:|
| positions only: the column of an instruction's point (`61:14` vs `61:6`: a call's receiver against the call, a getter's rhs against its name) | 47 | 6 | 1 | 1 | | 1 | **56** |
| positions with `names [(1, "this")]` and `scope` lines moved (the method's entry scope sits at the other column) | 43 | 3 | 8 | | | 1 | **55** |
| code (grouped by the first differing instruction, `tmp/r41/diff_classes.py`) | 138 | 53 | | 1 | 1 | | **193** |

Code groups, largest first [V shapes, causes I]: local `var` slots in another order (`%5 <digitDiff>`
vs `%5 <fractionDigits>`; 18 + 7 = **25**); a lambda's `$anonfun` method erased to `Object` where the
oracle keeps `java.io.FileDescriptor` (15 javalib lambdas, likely the 8 nativelib `call` diffs:
**23**); CFuncPtr0–22's `rawptr$accessor` not generated (**23**, nativelib, one cause); fresh
numbering `$anonfun$adapted$2` vs `$1`, `RawPtr`/`RawSize` constructors typed `ptr` (**18**); the
oracle boxes a value class (`classalloc ScalaOps$JavaIterableOps` then `__self`) where ours calls
`$extension` directly (**11**); anonymous classes initialise fields before the outer-accessor
call (**11**); `inlinehint def static CENATT(): int` constant getters the oracle keeps (**5**);
pattern-match type tests `as` + `ieq` against a bare `ieq` (**3**, 1,224 lines); ~55 others.

## 2. Verified mechanics of the batches

1. **Stubs.** nscplugin compiles javalib against the JDK, so its NIR names `java.time.Duration` and
   friends that no module defines; `libs/jdk-stubs.jar` (`just jdk-stubs`, scala-cli) is the
   measuring tool's JDK. The 25 files use few members: `Instant.{MIN, MAX, ofEpochMilli,
   ofEpochSecond, toEpochMilli}`, `Duration.{getSeconds, getNano}`, `TimeUnit`'s `toNanos` on it,
   `Locale.ROOT`; the rest appear in signatures and imports only [V grep]. nativelib's
   `runtime/Class.scala` needs `Field`, `Method`: `lib-compile.py` `OPTIONS` gives the stubs to
   javalib only. The stubs stay out of the VM's classpath (TASTy only) [V `Justfile:526-534`].
2. **Macros.** `scivolo-lower` makes a `VmMacros` runner only when `--cp` is given; `lib-compile`
   gives none. With a runner the macro's body comes from the published nativelib TASTy (3.1.3),
   whose quote format the macro runner refuses [V run]. The sbt build of the oracle leaves the
   3.9.0 TASTy beside the NIR (`tmp/lib/build/scala-native/nativelib/.3/target/scala-3.9.0/classes`,
   184 files) [V]; `lib-oracle.sh` can zip it per module and `lib-compile` open the Universe with
   those jars in place of `ref/libs`' (`SCIVOLO_LIBS` is already an option of `scivolo-lower`).
   Whether the macro then expands in the VM is untested [I]: a probe before the MR.
3. **`Local`.** `Yn/mod.rs:112` gives every private term `LOCAL` (dotty's inferred `private[this]`);
   `Y/access.rs:53` drops it when the member is used through a prefix that is not its owner's
   `this` (dotty's `canBeLocal`), but an `Ident` found through the companion's scope or an import
   has no prefix, and `is_privileged_prefix` counts the module reference `V` inside class `V` as
   privileged [V probes]. Dotty: `Typer.typedIdent` qualifies such an `Ident` with the module
   reference and `ExpandPrivate` makes the member non-private; `L/phases/expand_private.rs` does the
   second step already [V]. The 9 "inaccessible member" refusals sit at the two `Err` sites of the
   same file; `prefixLength: Integer` (`unset_local` on a parameter accessor reached through an
   implicit conversion's prefix) is the one verified shape [I for the others].
4. **Extern objects.** nscplugin (`P/NirGenStat.scala:626-660`) treats a body in an extern object as
   an extern *forwarder* when name and signature match the target's, a *runtime overload* when
   only the name does (`addAnnotation(NonExternClass); genMethod(dd)`), and `PrepNativeInterop`
   marks extension methods and implicit-class constructors `NonExtern` [V]. Nested objects are
   ordinary classes. `externs.rs` has the forwarder case; the other three fall to `unsupported`.

## 3. Batches

Stacked MRs for one coder, disjoint by file; order by files gained per line, each gated by
`just gates`, `just lib-compile MODULE` on its files without a worsened row (`just lib-baseline all`
first) and `just snapshot-diff programs lower typer`:

| MR | branch | §1.1 rows | gains | lines | owns | proves it |
|---|---|---|---:|---:|---|---|
| 1 | `lib-jdk-stubs` | JDK classes; macro | 25 (+5) | ~60 Scala, ~30 | `libs/jdk-stubs/JdkStubs.scala`, `libs/jdk-stubs.jar`, `scripts/lib-compile.py` (OPTIONS, nativelib gets the stubs), `scripts/lib-oracle.sh` (TASTy jars), `L/bin/scivolo-lower.rs` (a `--macros` switch) | `just lib-compile javalib` 523 ok; `just lib-compile nativelib runtime/Class`; the 5 macro files ok or a note why not |
| 2 | `lib-access` | companion privates; inaccessible | 12 (+9) | ~70 | `Y/access.rs`; one `tests/programs` entry (a companion-private value used from the class and through `import V._`, NIR identical) | `tmp/r41/probe/{local,ident}.scala` lower; `just lib-compile javalib` +12 at least; nativelib `regex/Compiler` |
| 3 | `lib-positions` | §1.3 rows 1–2 | 111 NIR files identical | ~60 | `L/codegen/positions.rs`, `L/codegen/method.rs` (entry scope), `L/phases/getters.rs:86-89` (the tpt span) | `just lib-compile windowslib` 130 same; `nir-cmp` on a 2-line case first (`java/io/PushbackInputStream`, `BufferedInputStream`) |
| 4 | `lib-externs-types` | extern objects; annotations and boxing except `@exported`; private type aliases | 4 + 3 + 6 = 13 | ~130 | `L/codegen/externs.rs`, `annotations.rs` (align group), `erased.rs` (box `RawPtr` as nscplugin's `Type.box`), `Y/dependent_results.rs`, `Y/select.rs:173` | `just lib-compile clib` 21 of 21; `nativelib runtime/GC`, `runtime/monitor`; javalib `zip/`, `ffi/zlib`, `lang/impl/` |

MR 1 is Scala and scripts, lands first and raises javalib to ~523; MR 2 is one typer file; MR 3
needs the probe before an estimate holds (one or three causes); MR 4 mixes two crates but no
file of 2 or 3. `@exported` (1 file, ~60 lines after `GenNativeExports.scala`) waits. Expected
after the four: nativelib ~97, clib 21, posixlib 65, javalib ~550, auxlib 6; identical NIR
≥ 2,676 of 2,869.

Following batches, in order: **auxlib** (8 files: `Yn/mod.rs:161` unenters Java-defined
classpath classes too; `BasicNode` across files needs the module as one unit or auxlib's own
TASTy, which the published jar lacks [V 0 `.tasty`]); **code-level NIR** — `var` order, lambda
result types, `rawptr$accessor`, fresh numbering (`L/codegen/method.rs`, `closure_methods.rs`,
`funcptr.rs`, `L/phases/closures.rs`); **wildcard receivers and the typer singles** (§1.2, with
`typer-compare` per file); the **lowering singles** with #165.

No sub-issues: four MRs stay under the brief's ~10-MR rule; #168 tracks them as a checklist.

## 4. The C parts and "the same artefacts"

What Scala Native ships per module (`_native0.5_3` jar, research 36 §1–2, ADR 0042) [V]: the
`.nir` files; the C/C++/assembly sources as resources under `scala-native/` (nativelib's GC,
`eh.cpp`, `delimcc`, `zone`; `.gyb`, `.properties`, `.modulemap` included), never compiled by
the Scala build — the toolchain compiles them with clang when a program is linked; `.class` and
`.tasty` files (javalib ships TASTy only as `javalib-tasty`); a manifest.

What the project has and needs:

| part | state | needed |
|---|---|---|
| NIR | per file, 714 of 841 sources, 2,565 of 2,869 files identical [V] | a module as one unit against its dependencies' jars (`L/units.rs:18` `compile_sources`, `scivolo-lower FILE...`) once every file of it lowers; fresh names per file (research 36 §5 item 10) |
| C sources | `native/` is the curated copy, `crates/snrt/build.rs` compiles it with `CC` (default `clang`) once per workspace build [V `build.rs:78`]; `just native-stale` lists drift against the jars' resources [V `scripts/native-stale.py`, 37 lines] | copying `src/main/resources/scala-native/**` into the module's jar verbatim: a zip step, no compiler |
| TASTy | upstream's (`libs/javalib-tasty-0.5.12.jar`, the published jars; the oracle build's 3.9.0 TASTy for MR 1) | a pickler (research 36 stage 4, ~1,200 lines) — the only large item |
| `.class` | none | none: the toolchain reads NIR only (`SN/project/Settings.scala:694-705` drops them where unneeded) |
| manifest | none | `META-INF/MANIFEST.MF` as sbt writes it: ~5 lines |

Recommendation: ADR 0042 stands — C stays resources, compiled by clang in `crates/snrt`,
no C compiler in scivolo's binaries; "the same artefacts" is NIR + resources + manifest, with
upstream TASTy until a pickler exists. The artefact step is one recipe, `just lib-jar MODULE`
(~80 lines of Python beside `lib-compile.py`: `scivolo-lower` over the module's three source
directories with `--sourceroot` each, zip NIR + resources + manifest, then `nir-cmp` of the jar
against `tmp/lib/sn/MODULE`), to be written when a module has no refusal left: clib (2) and
posixlib (2) reach that first, after MR 4 and the `signal`/`stat` singles. The backends to come
(#142, #170) are untouched: MRs 2 and 4's typer changes are dotty's own rules (`canBeLocal`,
dealiasing), the lowering changes live in the Native code generator (ADR 0043, 0045), and the
C and jar steps are per backend by construction.
