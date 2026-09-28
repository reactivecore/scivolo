# ADR 0008 — JDK behaviour where Scala Native falls short

Status: accepted · Date: 2026-09-28

## Context
Scala Native's javalib stubs or leaves out some JDK methods its native runtime cannot provide
cheaply; its test suite sometimes asserts the stub (`ThreadTest.currentThreadGetStackTrace`
expects an empty trace). The VM has information a native binary lacks: its own frames and the
collector's figures. Scripts written against the JDK expect its behaviour.

## Decision
- Where the VM can give JDK behaviour cheaply and a script would notice the difference, it
  overrides the javalib method (`crates/vm/src/natives/jdk.rs`):
  - `Thread.getStackTrace` of the current thread lists its frames as the JDK does, starting
    with `Thread.getStackTrace` itself; other threads keep javalib's empty array.
  - `Runtime.totalMemory` and `maxMemory` report the collector's heap size and limit;
    `freeMemory` reports 0, as the collector does not report live bytes.
- A VM native may define a method javalib lacks: virtual dispatch falls back to one when the
  class has no definition.
- A Scala Native test that asserts the stub then fails; it is listed as a known difference.

## Consequences
- Four more of dotty's `tests/run` pass (`i3006`, `i3006b`, `i2738`, `t8199`) and two that
  used `Runtime.totalMemory` (`t2849`, `t7336`); `ThreadTest.currentThreadGetStackTrace` fails.
- Each further case is judged the same way: a cheap override with visible benefit.
