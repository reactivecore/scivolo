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
just dotty-build    # ref/scala3 at 3.9.0 (sparse), each test compiled to NIR in tmp/dotty
just dotty-report   # run them; tmp/dotty/report.md, one line per test in tmp/dotty/results.tsv
just dotty-native   # link the tests failing in the VM as Scala Native binaries, for the report
```

A test passes when `Test.main` exits normally and its stdout matches the `.check` file, if any.
Skipped: tests for the JVM or Scala.js only, with Java sources, or using JVM-only APIs
(runtime reflection, class loading, processes, serialization). A test failing in the VM counts
as failing in Scala Native when its native binary fails too, or does not link.

Results (2026-09-28):

| Outcome | Tests |
|---|---:|
| passed | 1498 |
| failed in the VM only | 0 |
| fails in Scala Native too | 39 |
| compile failed | 4 |
| skipped | 170 |
| **total** | **1711** |

- **Fails in Scala Native too:** 17 crash, 13 print other output, 9 do not link. Reasons: Java
  reflection Scala Native lacks (`getConstructors`, `getEnclosingClass`, `getEnumConstants`,
  interface order), an empty `Thread.getStackTrace`, `Runtime.totalMemory`, and a crash in a
  SAM lambda (`i24573`, segfault natively). The VM prints the same output, except that its
  stack traces carry file and line.
- **Compile failed:** tests that need separate compilation or the JVM
  (`backwardsCompat-implicitParens`, `i19394`, `unroll-*`).
