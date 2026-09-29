# ADR 0008 — JDK behaviour where Scala Native falls short

Status: accepted · Date: 2026-09-28 · Amended 2026-09-29: implemented in the overlay (ADR 0009);
lambda bridges and `getInterfaces` added

## Context
Scala Native's javalib stubs or leaves out some JDK methods its native runtime cannot provide
cheaply; its test suite sometimes asserts the stub (`ThreadTest.currentThreadGetStackTrace`
expects an empty trace). The VM has information a native binary lacks: its own frames and the
collector's figures. Scripts written against the JDK expect its behaviour.

## Decision
- Where the VM can give JDK behaviour cheaply and a script would notice the difference, it
  overrides or adds the javalib method, in the library overlay (ADR 0009):
  - `Thread.getStackTrace` of the current thread lists its frames as the JDK does, starting
    with `Thread.getStackTrace` itself; other threads keep javalib's empty array.
  - `Runtime.totalMemory` and `maxMemory` report the collector's heap size and limit;
    `freeMemory` reports 0, as the collector does not report live bytes.
- Where Scala Native's compiler or linker produces something the JVM would not, and the VM can
  correct it at run time:
  - a virtual call of a method a class lacks calls a bridge the VM synthesizes as NIR, as the
    JVM's lambda factory adds bridges (`interp/bridge/`): among the class's methods of that
    name and arity whose argument and result types convert by boxing, unboxing, a cast or
    `unit`, the one needing fewest boxing conversions, called virtually; none if two tie.
    Unboxing `null` gives zero, as Scala's `BoxesRunTime` does. Scala Native's lambda classes
    do not cast their `Object` parameters, so an argument of the wrong type fails in the
    lambda's body, not with the JVM's `ClassCastException`;
  - `Class.getInterfaces` lists the direct interfaces in declaration order, without those the
    superclass or another direct interface extends, as Scala's JVM backend emits them.
- A Scala Native test that asserts the stub then fails; it is listed as a known difference.

## Consequences
- Four more of dotty's `tests/run` pass (`i3006`, `i3006b`, `i2738`, `t8199`) and two that
  used `Runtime.totalMemory` (`t2849`, `t7336`); `ThreadTest.currentThreadGetStackTrace` fails.
  With bridges and `getInterfaces`, `i24573` and `redundantParents` pass: every dotty test that
  fails in the VM fails in Scala Native too.
- Each further case is judged the same way: a cheap override with visible benefit.
