# Conformance

How the VM is checked against Scala Native's own test suites, and where it stands.

## Running

```sh
just fetch-libs                # once: Scala Native jars into ref/libs
scripts/sn-tests.sh all        # compile unit-tests (shared + native, JDK ≤ 17) with the JUnit
                               # plugin into tmp/sn-tests/nir; needs ref/scala-native at v0.5.12
scripts/sn-report.sh           # run them in the VM by group, print the table below
scripts/sn-run.sh Name -Other  # run test classes whose name contains `Name`, except `Other`
just ci-suite                  # all of it from scratch, as CI runs it on main (macOS arm64,
                               # Linux x86-64); the table is the job artifact tmp/sn-report/report.md
```

`tests/junit/runner.scala` is the test runner. The VM gives it the JUnit bootstrapper modules
through the externs in `crates/vm/src/natives/junit.rs`, because Scala Native's reflective
lookup needs metadata that its linker generates. Tests run from the root of `ref/scala-native`,
where their resource files are.

## Results (2026-09-29, macOS arm64 and Linux x86-64 in CI)

| Group | Passed | Failed | Skipped |
|---|---:|---:|---:|
| language (`scala.*`) | 237 | 0 | 1 |
| scala.scalanative | 737 | 1 | 0 |
| java.lang | 400 | 1 | 9 |
| java.io | 247 | 0 | 0 |
| java.net | 107 | 0 | 1 |
| java.math | 780 | 0 | 0 |
| java.nio | 2634 | 0 | 1 |
| java.util | 2651 | 1 | 2 |
| java.util.concurrent | 2934 | 0 | 6 |
| niobuffer | 546 | 0 | 0 |
| posixlib | 61 | 0 | 1 |
| runtime | 2 | 0 | 0 |
| **total** | **11336** | **3** | **21** |

Counts are test methods, from macOS. Linux fails the same tests. It skips four more: the
read-only-file cases, because the CI container runs as root.

A feature the VM lacks throws `java.lang.InternalError("scivolo: unsupported: …")`, a missing class
`NoClassDefFoundError`, a missing C function `UnsatisfiedLinkError`, so one gap fails a test
instead of the run.

## Known gaps behind the failures

- **Service providers.** `ServiceLoader` lists every provider `META-INF/services` names, so
  `ServiceLoaderTest.loadService`'s provider that is not opted in is found too (ADR 0007).
- **Stack traces of threads.** `Thread.getStackTrace` of the current thread lists its frames as
  on the JDK; `ThreadTest.currentThreadGetStackTrace` expects Scala Native's empty array
  (ADR 0008).
- **Object layout.** `IntrinsicsTest.sizeOfClassTest` expects the lock word multithreaded Scala
  Native adds to object headers; the VM keeps monitors outside objects (ADR 0003).
- **Static initializers.** Before `main`, Scala Native runs every `<clinit>` (of `@static`
  values) in the order of its linker's `Reach.done` hash map, which depends on how many
  definitions the program reaches, before the runtime's `init`; the VM runs the `<clinit>`s of
  the files before the library sorted by class name, after `init`, which the heap snapshot
  holds (#188). A dependency jar's file that replaces a library file counts as having a
  `<clinit>` only if the library's has one. Files added to a REPL session run theirs on first
  use of the class (ADR 0047).

## Upstream bugs to report

Found through the VM, not yet reported. Locations are in Scala Native v0.5.12 (`ref/scala-native`).
The VM fixes javalib bugs in its library overlay (`overlay/src`, ADR 0009).

1. **`LinkedTransferQueue.remove` reads `null.next`** (javalib,
   `java/util/concurrent/LinkedTransferQueue.scala:1214`).
   - Port of the JDK loop `for (Node c = p;; q = p.next)`: the update `q = p.next` also runs
     after the `break` that set `p = q = null`.
   - Removing an element the queue does not hold throws `NullPointerException` unless the
     optimizer drops the unused load; `LinkedTransferQueueTest$Generic.testRemoveElement`.
   - Fix: update `q` only when the loop continues. VM: fixed in the overlay.
2. **`ForkJoinPool.helpAsyncBlocker` fails on other threads** (javalib,
   `java/util/concurrent/ForkJoinPool.scala:2063`).
   - `Thread.currentThread() match { case wt: ForkJoinWorkerThread => … }` has no default
     case: `MatchError` where the JDK does nothing.
   - Reached from `SubmissionPublisher` when a producer blocks outside the pool, which green
     threads make likely; 5 `SubmissionPublisherTest` cases.
   - Fix: `case _ => ()`. VM: fixed in the overlay.
3. **`HashSet.containsAll`'s lambda throws** (javalib, `java/util/HashSet.scala:31`).
   - `c.iterator().scalaOps.forall(contains)` on a `Collection[_]` eta-expands `contains` with a
     `Nothing` parameter; the published javalib's lambda body is `throw o`.
   - Fix: `forall(e => contains(e))`. VM: `containsAll` is replaced in the overlay.
4. **A SAM lambda lacks its bridge** (nscplugin's lambda classes; dotty
   `tests/run/i24573.scala`).
   - For `trait ToInt[-T] extends (T => Int) { def apply(t: T): Int }`, the lambda class of
     `(s: String) => s.length` defines `apply(Object): Int` only; a call through `Function1`
     needs `apply(Object): Object`, which the JVM's lambda factory adds as a bridge.
   - The Scala Native binary segfaults; minimal reproduction: `tests/programs/bridges`.
   - Fix: emit the bridges of the functional interface's erased signatures. VM: synthesizes the
     bridge on dispatch (ADR 0008).
5. **No static forwarder beside a `@static` method of the same name** (nscplugin,
   `scala-3/scala/scalanative/nscplugin/NirGenStat.scala:835`).
   - An object with `@static def foo` and an instance method `foo` of the same signature (e.g.
     through `@targetName("foo")`) fails to compile: "found existing public static method …
     cannot generate a static forwarder".
   - Scala 3's JVM backend skips forwarders whose name the companion class already has (dotty
     `tests/run/i19394.scala`).
   - Fix: skip the forwarder instead of reporting an error. VM: not affected.
6. **Static forwarders of inherited methods carry positions in the wrong file** (nscplugin,
   `scala-3/scala/scalanative/nscplugin/NirGenStat.scala:826`).
   - The forwarder's position is the member's span read against the current unit's source
     (`NirPositions.fromSpan`), even when the member is defined in another file.
   - auxlib's `scala.scalanative.runtime.BoxedUnit` forwarders to `equals`/`hashCode`/`toString`
     of `scala/runtime/BoxedUnit.scala` point into `scala/scalanative/runtime/BoxedUnit.scala`,
     past its end.
   - Fix: convert `sym.sourcePos`. VM: positions in the member's own file.
7. **`-language:experimental.X` is named twice in `@experimental`** (dotty,
   `compiler/src/dotty/tools/dotc/typer/Checking.scala:964`).
   - `checkAndAdaptExperimentalImports` writes `-language:experimental.$sel` where `sel` already
     is `experimental.X`: `@experimental("Added by -language:experimental.experimental.erasedDefinitions")`.
   - Fix: `-language:$sel`. Frontend: the same text as dotty.
8. **Stale `.check` files in `tests/run-macros`** (dotty 3.9.0, `tasty-extractors-1`,
   `tasty-extractors-2`, `tasty-extractors-types`, `type-print`).
   - The checks print package prefixes as `ThisType(TypeRef(NoPrefix(), "immutable"))`; 3.9.0
     on Scala Native and on the JVM prints the pickled `TermRef` paths. `tasty-extractors-2` also
     expects `n.toString` in `productElement`, `type-print` other highlight colours.
   - Fix: re-record the checks. VM: compared with `tests/dotty-checks/run-macros`, recorded from
     dotty 3.9.0 on Scala Native.
9. **Anonymous mirrors in inline bodies lack `fromProduct`** (dotty 3.9.0,
   `compiler/src/dotty/tools/dotc/typer/Synthesizer.scala:277`, `anonymousMirror`).
   - `inline def mk(p: Product): Some[Int] = summon[Mirror.ProductOf[Some[Int]]].fromProduct(p)`:
     each inlined copy of the anonymous mirror class calls a `fromProduct` it does not define.
   - Scala Native: "Unknown method `…$anon$1.fromProduct(scala.Product): scala.Some`" at link
     time; JVM: `NoSuchMethodError`.
   - Fix: give the inlined anonymous mirror classes their mirror members. Frontend: defines them.
10. **A trait with an `extension` group has `NoInits` only when read from TASTy** (dotty 3.9.0,
    `compiler/src/dotty/tools/dotc/ast/TreeInfo.scala:485`, `defKind`).
    - `Namer` takes the flag from `untpd.bodyKind` of the unexpanded body, where `ExtMethods`
      counts as an initializer; `TreeUnpickler.indexStats` sees only methods and sets `NoInits`.
    - A class extending the trait calls its empty `$init$` when compiled with the trait, not
      against its TASTy: nativelib's `unsafe/package$` and `UnsafePackageCompat`.
    - Fix: `defKind` of `ExtMethods` as of its methods. Frontend: as dotty; `just lib-compile
      nativelib` reads `UnsafePackageCompat` from TASTy until #196, so `unsafe/package$` lacks the
      call and `unsafe/package`'s forwarders to its members have the object's position, as in
      separate compilation.
11. **`super[Q].m` of a `@targetName` member crashes `ResolveSuper`** (dotty 3.9.0,
    `compiler/src/dotty/tools/dotc/transform/ResolveSuper.scala:89`, `decomposeSuperName`).
    - Trait `P extends Q` with `@targetName("tn") def n` calling `P.super[Q].n` from a nested
      class: the accessor's target name `tn$super$Q` is a `SimpleName`, so neither case matches
      (`MatchError`).
    - Fix: decompose the target name's string. Frontend: compiles it, calling `Q`'s `tn`.
12. **`multiSpreads` crash on nested spreads and in inline expansions** (dotty 3.9.0,
    `compiler/src/dotty/tools/dotc/transform/PostTyper.scala:473,774`, `flattenSpreads`).
    - `f(f(1, xs*), xs*)`: the outer literal is flattened without transforming its elements, so
      the inner `<special-ops>.spread` reaches the backend (`SI-5604` assertion).
    - `inline def g(inline ys: List[Int]) = f(0, ys*, 9)` called as `g(next())`: the lifted
      `ev$N` is owned by the enclosing class, and `LambdaLift` finds no proxy for it.
    - Fix: transform the elements; lift with the owner of the expansion. Frontend: compiles both,
      evaluating each spread once, in order.

## M4: dotty `tests/run`

```sh
just dotty-build    # ref/scala3 at 3.9.0 (sparse), each test compiled to NIR under tmp/dotty
just dotty-report   # run them; report.md and results.tsv (one line per test) under tmp/dotty
just dotty-native   # link the tests failing in the VM as Scala Native binaries, for the report
```

A test passes when `Test.main` exits normally and its stdout matches the `.check` file, if any.
As in dotty, a directory's files are compiled in groups by the `_N` suffix of their names, in
ascending order, each group seeing the output of the ones before it. Skipped: tests for one
platform, with Java sources, compiled partly by another compiler (a `_c3.0.2` suffix), or using
APIs the Scala Native javalib lacks (member reflection, processes, serialization). A VM failure
counts as failing in Scala Native too when the native binary fails the same way with the same
stdout, or does not link for a missing definition. A run with a filter updates only its tests'
rows.

Results (2026-09-29):

| Outcome | Tests |
|---|---:|
| passed | 1504 |
| wrong output | 0 |
| crashed | 0 |
| fails in Scala Native too | 31 |
| compile failed | 1 |
| skipped | 173 |
| **total** | **1709** |

- **Fails in Scala Native too:** reflection Scala Native lacks (`getConstructors`,
  `getEnclosingClass`, `getEnumConstants`) and a `main` returning a value (`t4536`). Tests that
  need `Thread.getStackTrace` or `Runtime.totalMemory` pass in the VM, as do `i24573` (a
  missing lambda bridge) and `redundantParents` (`getInterfaces`) (ADR 0008).
- **Compile failed:** `i19394`, in Scala Native's compiler plugin (upstream bug 5).

### Through the Rust frontend

```sh
just dotty-lower-report   # typer, lowering and VM; lower-report.md and lower-results.tsv under tmp/dotty
```

Each single-file test is lowered by `scivolo-lower` and run in the VM. Tests failing in the VM when
compiled by Scala Native (`dotty-report`) are counted apart; refusals are grouped by the reason
the typer or the lowering gives. The programs `tests/programs/lowered.txt` lists run as
`scivolo FILE.scala` with `SCIVOLO_FRONTEND=strict` (`crates/scivolo/tests/lowered.rs`).

Results (2026-11-08):

| Outcome | Tests |
|---|---:|
| passed | 1494 |
| wrong output | 0 |
| crashed | 0 |
| timeout | 0 |
| lowering refused | 0 |
| typer refused | 12 |
| parser refused | 0 |
| macro refused | 0 |
| frontend crashed | 0 |
| fails in the VM | 32 |
| skipped | 173 |
| **total** | **1711** |

- **Typer refused:** one each, mostly `experimental.modularity` (`tracked` parameters, context
  bounds of `Self` types), `typeCheckErrors` of a non-constant, implicit search with local
  implicits of an inline body, constructor proxies of exported classes. `Runtime.totalMemory`,
  `maxMemory` and `freeMemory`, which Scala Native's javalib lacks and the overlay adds (ADR
  0008), are typed through `libs/jdk-stubs/JdkMemberDeclarations.scala`.
- **`-source future`:** typed as 3.9 with the parser's new-syntax rules; the typer refuses
  alphanumeric infix calls, types and extractor patterns not declared `infix` (also those of
  libraries compiled before 3.4, which dotty accepts) and checks no other `future` rule.
- **Capture checking:** typed as dotty's typer types it (`ImpureFunctionN`, `@retains` trees);
  types drop `@retains`, `@retainsCap` and `@retainsByName` as unpickled ones do (case 1: the
  annotations are not refining and erase away), and the capture checker, which runs after the
  typer and leaves the NIR unchanged, is not run (#168). Quotes under capture checking, whose
  TASTy would keep the annotations, are refused.

### com-lihaoyi libraries from source

`just lib-source NAME` compiles each file of a library on its own and compares its NIR with
scala-cli's build of the whole module and, for the files that differ, with scala-cli's build of
that file alone. Differences from both (2026-11-04):

- **Quote type sharing** (fastparse `MacroInlineImpls$`, `MacroRepImpls$`, upickle-implicits
  `macros$package$`; case 1): a quote string writes a type it wrote before as a `SHAREDtype`
  where dotty writes it again, or the reverse. dotty shares types by object identity: a class
  reference read from TASTy (by name), one from a JDK class file (by symbol through the package's
  `this`) and an implicit-scope companion through its class's prefix are different objects;
  ours tells references apart by symbol, name and `this` only. The quote reads back as the same
  trees.

Against the whole-module build alone, sourcecode `Macros$`, mainargs `Macros$` and pprint
`package` differ (case 1: types shared across files, static forwarders of members a package
object inherits); compiled alone, dotty gives ours, mainargs' up to quote positions. upickle's
own 35 files have no scala-cli oracle (Scala 3.9 reports cyclic references in `Api.scala`).

`just lib-compile scalalib` lowers each file of Scala Native's scalalib alone against Scala's
library, its `LazyVals` replaced by Scala Native's (`scripts/scalalib_sources.py`). `Equiv$TupleNEquiv`
and `Ordering$TupleNOrdering` (N = 2…9, open, not accepted) differ from Scala Native's build of
the whole module in `(a, b).hashCode()`: the build calls `Any.hashCode`, `(Object) => int`, since a
case class's synthetic members do not exist yet while a class of the same run is typed; ours reads
`Tuple2` from TASTy and calls `Tuple2.hashCode`, `(Tuple2) => int`, as Scala Native does for a user
program. Also open, not accepted, in a user program: a value of type `Array[Nothing]` or
`Array[Null]` is `array[nothing]` or `array[null]` in Scala Native's NIR, ours
`as[array[@java.lang.Object]]` of it; and
`this.asInstanceOf[S]` returned from a method of a trait with self type `S` casts at the type
argument's position in Scala Native's NIR, at `this` in ours.

## M5: scala/scala `test/junit`

```sh
just m5-build    # ref/scala2 at v2.13.18 (sparse), compiled with Scala 3.9 into tmp/m5
just m5-report   # run each test class; report.md and results.txt under tmp/m5
```

Since Scala 3.8 the standard library is compiled by Scala 3, so scala/scala's JUnit tests for
it (v2.13.18, the version it is based on) are compiled with Scala 3.9 and Scala Native's JUnit
plugin, minus the files Scala Native's denylist for 2.13.18 excludes. Files that do not compile
with Scala 3 are dropped until the rest does; `AssertUtil` gets one Scala 3 fix. Each test
class runs in its own VM, 900 s at most.

Results (2026-09-28): of 104 files, 92 compile; all 867 tests in their 112 classes pass.

- **Not compiling with Scala 3 (12 files):** ambiguous givens (`BuildFromTest`,
  `DecoratorsTest`), missing `ClassTag`s, `==` between unrelated types, and overloads that
  Scala 3 resolves differently (`SortingTest`, `ArraySortingTest`, `RicherTest`).
- **Slowest:** `ListSetTest.hasTailRecursiveDelete` builds a 50,000-element `ListSet`, a
  quadratic 1.25 billion `==` on boxed `Int`s.
