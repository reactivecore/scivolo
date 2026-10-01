# ADR 0010 — The lowering ports dotty's post-typer phases faithfully

Status: accepted · Date: 2026-10-01

## Context
The own frontend's NIR must equal what dotc and Scala Native's compiler plugin make from the same
source (ADR 0001): the VM, the script cache and the oracle compare names, signatures and bodies.
nscplugin names everything from the symbols as dotty's phases after the typer left them, and those
phases also decide which closures, classes and calls exist (research 10 §2).

## Decision
- `crates/lower` ports the post-typer phases the scripting subset needs, line by line and as
  sequential passes over the typer's trees in dotty's order, including phases that only remove
  sugar; MegaPhase fusion is not reproduced.
- It ports nscplugin's NIR generation without native interop.
- What the typer leaves is what dotc's `-Ystop-after:typer` shows; the typer also hands on the
  unit's fresh-name counters (research 10 §1).

## Consequences
- About 18k lines of Scala to port (research 10 §6); each phase's result is checked by the NIR
  oracle, not by dotty's `-Ycheck`.
- Symbols need an info per phase where later phases read an earlier one (`atPhase`).
- Simplifying a phase is allowed only where the oracle shows the same NIR.
