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

A feature the VM lacks throws `java.lang.InternalError("scalai: unsupported: …")`, a missing class
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

Each single-file test is lowered by `lower` and run in the VM. Tests failing in the VM when
compiled by Scala Native (`dotty-report`) are counted apart; refusals are grouped by the reason
the typer or the lowering gives.

Results (2026-10-09):

| Outcome | Tests |
|---|---:|
| passed | 213 |
| wrong output | 1 |
| crashed | 1 |
| lowering refused | 129 |
| typer refused | 1089 |
| parser refused | 13 |
| several files | 59 |
| fails in the VM | 31 |
| skipped | 175 |
| **total** | **1711** |

- **Wrong output:** i9939, numeric harmonization in the typer (#60).
- **Crashed:** t8893, tail recursion (#59).
- **Lowering refused:** 47 reasons, none above 8 tests (nested classes and objects,
  Enumeration values without names #62).
- **Typer refused:** mostly overloads with default or by-name parameters (244) and inline
  definitions and calls (125).

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
