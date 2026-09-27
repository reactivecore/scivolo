# Conformance

How the VM is checked against Scala Native's own test suites, and where it stands.

## Running

```sh
just libs                      # once: Scala Native jars into ref/libs
scripts/sn-tests.sh all        # compile unit-tests (shared + native, JDK ≤ 17) with the JUnit
                               # plugin into tmp/sn-tests/nir; needs ref/scala-native at v0.5.12
scripts/sn-report.sh           # run them in the VM by group, print the table below
scripts/sn-run.sh Name -Other  # run test classes whose name contains `Name`, except `Other`
```

`tests/junit/runner.scala` is the test runner. The VM gives it the JUnit bootstrapper modules
through the externs in `crates/vm/src/natives/junit.rs`, because Scala Native's reflective
lookup needs metadata that its linker generates. Tests run from the root of `ref/scala-native`,
where their resource files are.

## Results (2026-09-27, with the Immix collector and stack traces)

| Group | Passed | Failed | Skipped |
|---|---:|---:|---:|
| language (`scala.*`) | 237 | 0 | 1 |
| scala.scalanative | 735 | 0 | 3 |
| java.lang | 363 | 18 | 16 |
| java.io | 200 | 0 | 4 |
| java.math | 779 | 1 | 0 |
| java.net (earlier run, not in `sn-report.sh`) | 72 | 1 | 1 (run stops: blocking socket test) |
| java.nio | 2621 | 13 | 1 |
| java.util | 2560 | 6 | 8 |
| java.util.concurrent | 143 | 0 | 80 |
| niobuffer | 546 | 0 | 0 |
| posixlib | 60 | 1 | 1 |
| runtime | 2 | 0 | 0 |
| **total** | **8318** | **40** | **115** |

`ProcessTest` waits up to 60 s per case, so `just sn-report` gives each group 300 s.

A feature the VM lacks throws `java.lang.InternalError("scalai: unsupported: …")`, a missing class
`NoClassDefFoundError`, a missing C function `UnsatisfiedLinkError`, so one gap fails a test
instead of the run.

## Known gaps behind the failures

- **Threads.** The VM runs one thread and reports multithreading disabled; 80 concurrent tests
  skip, blocking socket tests hang, and process tests that wait with timeouts fail.
- **Class metadata.** `Class.forName` and other reflection needing linker metadata.
- **Environment.** A few `System`/`ProcessTest` cases (environment variables, `user.dir`,
  git subprocess with timeouts) depend on threads or on how the test harness starts the process.
- **javalib miscompiles.** Some javalib lambdas take a `Nothing` parameter and compile to
  `throw o` (e.g. `HashSet.containsAll`); the VM throws `ClassCastException` there, as a JVM
  would, and the affected tests fail.
