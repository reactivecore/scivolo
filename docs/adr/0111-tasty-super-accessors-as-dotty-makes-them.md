# ADR 0111 — The TASTy pickler makes every super accessor dotty's `SuperAccessors` makes

Status: accepted · Date: 2026-11-09 · Amends ADR 0105

## Context
ADR 0105 refuses a unit whose lowering makes super accessors the TASTy lacks, and the super
calls of methods with a target name. dotty's `SuperAccessors.transformSelect` also gives a
trait an accessor for `C.this.m` of a protected `m` that a class nested in it reaches
(`protectedSuper`); `deriveTargetNameAnnotation` gives the accessor of a member with a target
name the annotation `@targetName` of the accessor name of that target. Neither accessor had a
position.

## Decision
- The pickler makes the accessors of `C.this.m` as `transformSelect` does
  (`needsAccessorIfNotInSubclass`, `hostForAccessorOf`), so the generator's check that the
  TASTy declares every accessor the lowering made passes.
- The accessor of a member with target name `t` gets as target name the accessor name of `t`
  (`super$t`, `t$super$M`, a trait's expanded), in its signed name and in the lowering, and the
  annotation `@targetName("…")` after its flags.
- An accessor's definition and result type get the point of its call (`sel.span.focus`), the
  `this` of the call the call's start. Trees without a node are registered under ids from
  `u32::MAX / 2` and written before the node that follows them in the trees.

## Consequences
- The JVM and NIR names of these accessors are their target names (`B1$$super$f2`), as for
  every member with a target name.
- Open still: the position of the `@targetName` annotation (dotty gives it the member's span)
  and `SHAREDtype` by identity (ADR 0105).
