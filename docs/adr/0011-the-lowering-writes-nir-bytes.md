# ADR 0011 — The lowering writes NIR bytes

Status: accepted · Date: 2026-10-01

## Context
The lowering could hand the VM decoded definitions (`World::define`) or write NIR files. The VM
finds a class's members through its file's index, which `define` does not extend, and the script
cache stores compiled scripts as files (research 10 §4).

## Decision
- `crates/nir` gets a writer, a port of Scala Native's `BinarySerializer`, that the decoder reads
  back.
- The lowering writes one `.nir` file per top-level name, as nscplugin does; the VM and the script
  cache read them like any compiled NIR.

## Consequences
- One path into the VM for dotc's NIR and the own frontend's.
- The oracle compares decoded definitions, since nscplugin's bytes carry positions and paths.
