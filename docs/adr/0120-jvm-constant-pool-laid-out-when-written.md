# ADR 0120 — On the JVM, the constant pool is laid out when the class file is written

Status: accepted · Date: 2026-11-15

## Context
dotty builds a `ClassNode` holding names and values only; ASM's `ClassWriter` adds the pool
entries in the order `ClassNode.accept` visits them (the class, its attributes' values, the
inner classes, fields, then each method's header, code, locals and frames) and the attribute
names last (`toByteArray`). An `ldc` is `ldc_w` once its constant's index passes 255, so the
order decides the code. Our generator writes code with pool indices as it goes, interleaving
fields and methods, and knows the inner classes only after all code.

## Decision
- `ClassFile` keeps attributes by their names and values and each method's `Code` unfinished;
  the code refers to a pool of its own.
- `ClassFile::bytes` lays out a new pool in ASM's order: it adds the class's entries, then for
  each method its header's, its code's in instruction order (`Code::remap`, which changes an
  `ldc`'s form and moves the offsets after it), its locals' names, then computes the stack map
  frames, whose classes come last; the attribute names follow in ASM's order of writing.
- The frames, jump widening and code checks run in `bytes`, with one `Lub` per class.
- The generator may add members in its own order and moves them to dotty's
  (`ClassFile::move_methods`): `<clinit>` after the constructor, a trait method's static
  forwarder before it.

## Consequences
- Pool indices of the code's pool are not the class file's: a test reads them from the bytes.
- Methods' code stays in memory until the class is written.
- Entries the code's pool holds but no instruction refers to are not written.
