# ADR 0095 — Library builds see the JDK 17 API

Status: accepted · Date: 2026-11-08 · Extends ADR 0082

## Context
Scala Native's build compiles its libraries against JDK 17. javalib 0.5.12 defines members of
later JDKs, such as `ExecutorService.close()` (JDK 19) and the `SequencedCollection` methods
(JDK 21). Seen through javalib, a class of scalalib mixing in `ExecutorService` gets a forwarder
to `close()` that Scala Native's build lacks (`ExecutionContextImpl$$anon$4`, #219 item 1).

## Decision
- `scivolo-lower --jdk-17` makes the compile see the JDK 17 API; `just lib-compile` and
  `just lib-jar` pass it for every module but javalib, whose own classes are Scala classes.
- Under it, `mixin` makes no forwarder to a method of a javalib JDK class that JDK 17 lacks.
- `scripts/jdk_17_view.py` generates the lacking members, by class and method name: javalib's
  methods of a class JDK 17 or 21 has whose name JDK 17's class lacks
  (`crates/lower/src/symbols/jdk_17_missing.rs`).
- A member is matched by name; the script fails if javalib has an overload JDK 17 lacks of an
  interface method name JDK 17 has.
- User programs keep javalib's full API.

## Consequences
- Typing still sees javalib's members and parents; a library source calling a member JDK 17
  lacks compiles where Scala Native's build would not.
