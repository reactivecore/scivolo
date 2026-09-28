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

## Results (2026-09-28, macOS arm64 and Linux x86-64 in CI)

| Group | Passed | Failed | Skipped |
|---|---:|---:|---:|
| language (`scala.*`) | 237 | 0 | 1 |
| scala.scalanative | 737 | 1 | 0 |
| java.lang | 393 | 8 | 9 |
| java.io | 247 | 0 | 0 |
| java.net | 107 | 0 | 1 |
| java.math | 780 | 0 | 0 |
| java.nio | 2634 | 0 | 1 |
| java.util | 2651 | 1 | 2 |
| java.util.concurrent | 2928 | 6 | 6 |
| niobuffer | 546 | 0 | 0 |
| posixlib | 61 | 0 | 1 |
| runtime | 2 | 0 | 0 |
| **total** | **11323** | **16** | **21** |

Counts are test methods, from macOS. Linux fails the same tests. It skips four more: the
read-only-file cases, because the CI container runs as root. `SubmissionPublisherTest.testCancelledConsume`
fails now and then on either platform.

A feature the VM lacks throws `java.lang.InternalError("scalai: unsupported: …")`, a missing class
`NoClassDefFoundError`, a missing C function `UnsatisfiedLinkError`, so one gap fails a test
instead of the run.

## Known gaps behind the failures

- **Class metadata and resources.** `Class.forName`, `ServiceLoader` and
  `getResourceAsStream` need metadata or embedded resources Scala Native's linker generates.
- **javalib bugs.** `ForkJoinPool.helpAsyncBlocker` matches only worker threads (5
  `SubmissionPublisher` cases), and one `LinkedTransferQueue` case. The published javalib also
  compiled `HashSet.containsAll`'s lambda to `throw o`; the VM replaces that lambda.
- **Object layout.** `IntrinsicsTest.sizeOfClassTest` expects the lock word multithreaded Scala
  Native adds to object headers; the VM keeps monitors outside objects (ADR 0003).
- **Single cases.** `Math.multiplyHigh`.

## M4: dotty `tests/run`

```sh
just dotty-build    # ref/scala3 at 3.9.0 (sparse), each test compiled to NIR under tmp/dotty
just dotty-report   # run them; report.md and results.tsv (one line per test) under tmp/dotty
just dotty-native   # link the tests failing in the VM as Scala Native binaries, for the report
```

A test passes when `Test.main` exits normally and its stdout matches the `.check` file, if any.
Skipped: tests for one platform, with Java sources, or using APIs the Scala Native javalib
lacks (member reflection, processes, serialization). A VM failure counts as failing in Scala
Native too when the native binary fails the same way with the same stdout, or does not link
for a missing definition. A run with a filter updates only its tests' rows.

Results (2026-09-28):

| Outcome | Tests |
|---|---:|
| passed | 1497 |
| wrong output | 1 |
| crashed | 1 |
| fails in Scala Native too | 37 |
| compile failed | 4 |
| skipped | 169 |
| **total** | **1709** |

- **Fails in Scala Native too:** reflection Scala Native lacks (`getConstructors`,
  `getEnclosingClass`, `getEnumConstants`), an empty `Thread.getStackTrace`,
  `Runtime.totalMemory`, and a `main` returning a value (`t4536`).
- **Failing in the VM:** `i24573` (a SAM lambda without its `apply(Object, Object)` bridge;
  segfaults natively) and `redundantParents` (`getInterfaces` lists all interfaces, in another
  order than natively). Both fail natively too, differently (#20).
- **Compile failed:** tests that need separate compilation (#21).

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
