# ADR 0101 — A JVM backend in its own crates, behind a `Backend` switch

Status: accepted · Date: 2026-11-05 · Its JDK consequence done by ADR 0106 and 0107

## Context
The roadmap lists JVM bytecode as optional (Phase 5). Research 46 shows that everything up to
dotty's `SelectStatic` is shared with the NIR path; `GenBCode` needs the lowered trees, which NIR
no longer has (generic signatures, `invokespecial`/`invokedynamic`, inner classes). Neither
`noak` nor `ristretto_classfile` computes `max_stack`, jump offsets or stack map frames.

## Decision
- `crates/classfile` (`classfile`): an own class file model and writer, with no dependency on our
  crates; later the reader.
- `crates/jvm` (`jvmgen`, bin `scivolo-jvm`): a port of dotty's `GenBCode` from the lowered trees.
- `crates/lower` stays the home of the phases: `lower_trees` runs them and hands the lowered
  `Lower` to a generator; `compile_units` is the frontend with that generator in place of NIR
  generation. NIR generation moves out to `crates/nirgen` once the JVM generator shows the seam.
- Phases whose JVM shape differs (`LazyVals`, `MoveStatics`, `CompleteJavaEnums`, the native
  interop) get a `Backend { Native, Jvm }` in `Lower` when the JVM generator first needs them.
- The oracle is scala-cli 3.9.0's class files through `javap` (`just jvm-check`); output is
  semantically equivalent, not byte-identical.

## Consequences
- The NIR path is unchanged; `lower_sources` and `compile_sources` are the NIR instances.
- The JVM backend reads JDK members through javalib's TASTy and the JDK declarations until the
  class file reader exists; a member javalib alone has fails on the JVM.
