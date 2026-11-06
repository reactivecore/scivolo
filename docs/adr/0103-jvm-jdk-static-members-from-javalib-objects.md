# ADR 0103 — JDK static members on the JVM from javalib's objects

Status: accepted · Date: 2026-11-06

## Context
Until the class file reader exists (research 46 §7 step 10), the JVM backend sees a JDK class's
static members as members of javalib's object for it, or of the JDK declarations over it (ADR
0072). javalib declares some of the JDK's static fields as parameterless methods
(`def out: PrintStream` for `System.out`), so the symbol does not say whether the JDK has a field
or a method.

## Decision
- A member of an object standing for a JDK class (`is_jdk_object`) is a static member of that
  class: `invokestatic` of the class without `$`, an interface method if its companion is one.
- Such a member that is a `val` or a `def` without parameter clauses is the JDK's static field
  (`getstatic`): every Java method has a parameter list.

## Consequences
- A javalib object member the JDK lacks, or declares differently, fails on the JVM with
  `NoSuchMethodError` or `NoSuchFieldError` until the reader replaces this rule.
