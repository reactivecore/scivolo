# ADR 0103 — JDK static members on the JVM from javalib's objects

Status: accepted · Date: 2026-11-06

## Context
Until the class file reader exists (research 46 §7 step 10), the JVM backend sees a JDK class's
static members as members of javalib's object for it, or of the JDK declarations over it (ADR
0072). javalib declares some of the JDK's static fields as parameterless methods
(`def out: PrintStream` for `System.out`), some static methods without a parameter list
(`def values` of `PosixFilePermission`), and public values the JDK lacks
(`ProcessBuilder.nullFile`), so the symbol does not say whether the JDK has a field or a method.

## Decision
- A member of an object standing for a JDK class (`is_jdk_object`) is a static member of that
  class: `invokestatic` of the class without `$`, an interface method if its companion is one.
- A `val` of the JDK declarations (`@static`) or of a Java class entered from the library jars is
  the class's static field (`getstatic`).
- A parameterless javalib member is a static field if a table of the JDK 17 classes' public
  static fields (`jdk_statics.rs`) lists it with its descriptor; a `def` it does not list is a
  static method; other values, and listed ones of another descriptor, are refused.

## Consequences
- A javalib method the JDK lacks, or declares with another descriptor, fails on the JVM with
  `NoSuchMethodError` until the reader replaces this rule.
- A JDK static field javalib reaches through a class without a top-level object of its own is
  refused.
