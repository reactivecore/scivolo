# ADR 0067 — Quotes pickled by an own TASTy writer, byte-identical to dotty's

Status: accepted · Date: 2026-11-01

## Context
A quote outside a splice compiles to `unpickleExprV2(<TASTy as Base64 strings>, …)`; the strings
are part of the NIR, so NIR identity with Scala Native means every TASTy byte (research 43).
Our `crates/tasty` only read TASTy. An own encoding of quotes (research 43, option C) breaks
cached library images and `--dep` jars.

## Decision
- `crates/tasty/src/write/` ports dotty's writer as is: `TastyBuffer`, `NameBuffer` with its
  registration order, `TreeBuffer.compactify` with its 1 % stop rule, `TastyPickler`'s header
  and `pjwHash64` UUID. The tooling string is `Scala 3.9.0` (ADR 0046's oracle).
- The tree and position picklers build on it (research 43, steps 3–5).

## Consequences
- Our reader checks the writer: re-encoding a decoded file reproduces its bytes.
- A TASTy format change of a later Scala version is followed in both directions.
