# Conformance

How the VM is checked against Scala Native's own test suites, and where it stands.

## Running

```sh
just fetch-libs                # once: Scala Native jars into ref/libs
scripts/sn-tests.sh all        # compile unit-tests (shared + native, JDK ≤ 17) with the JUnit
                               # plugin into tmp/sn-tests/nir; needs ref/scala-native at v0.5.12
scripts/sn-report.sh           # run them in the VM by group, print the table below
scripts/sn-run.sh Name -Other  # run test classes whose name contains `Name`, except `Other`
just ci-suite                  # all of it from scratch, as CI runs it on main (Linux x86-64);
                               # the table is the job artifact tmp/sn-report/report.md
```

`tests/junit/runner.scala` is the test runner. The VM gives it the JUnit bootstrapper modules
through the externs in `crates/vm/src/natives/junit.rs`, because Scala Native's reflective
lookup needs metadata that its linker generates. Tests run from the root of `ref/scala-native`,
where their resource files are.

## Results (2026-09-27, with green threads, the Immix collector and stack traces)

| Group | Passed | Failed | Skipped |
|---|---:|---:|---:|
| language (`scala.*`) | 237 | 0 | 1 |
| scala.scalanative | 737 | 1 | 0 |
| java.lang | 390 | 11 | 9 |
| java.io | 247 | 0 | 0 |
| java.net | 107 | 0 | 1 |
| java.math | 780 | 0 | 0 |
| java.nio | 2634 | 0 | 1 |
| java.util | 2651 | 1 | 2 |
| java.util.concurrent | 2928 | 6 | 6 |
| niobuffer | 546 | 0 | 0 |
| posixlib | 60 | 1 | 1 |
| runtime | 2 | 0 | 0 |
| **total** | **11319** | **20** | **21** |

Counts are test methods; before threads, the concurrent group skipped its 80 classes whole.
The concurrent group needs about 20 minutes.

A feature the VM lacks throws `java.lang.InternalError("scalai: unsupported: …")`, a missing class
`NoClassDefFoundError`, a missing C function `UnsatisfiedLinkError`, so one gap fails a test
instead of the run.

## Known gaps behind the failures

- **Class metadata and resources.** `Class.forName`, `ServiceLoader` and
  `getResourceAsStream` need metadata or embedded resources Scala Native's linker generates.
- **Environment.** `System.getenv` and `user.dir` cases depend on how the test harness starts
  the process.
- **javalib bugs.** `ForkJoinPool.helpAsyncBlocker` matches only worker threads (5
  `SubmissionPublisher` cases), and one `LinkedTransferQueue` case. The published javalib also
  compiled `HashSet.containsAll`'s lambda to `throw o`; the VM replaces that lambda.
- **Object layout.** `IntrinsicsTest.sizeOfClassTest` expects the lock word multithreaded Scala
  Native adds to object headers; the VM keeps monitors outside objects (ADR 0003).
- **Single cases.** `Math.multiplyHigh` and one `dlopen` test.

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
