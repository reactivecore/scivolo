# ADR 0085 — The quote pickler restores dotty's tree shapes the typed trees lack

Status: accepted · Date: 2026-11-03

## Context
Byte-identical quotes (ADR 0067) with class and object definitions depend on how dotty's trees
look when `PickleQuotes` runs, not only on the typed trees our typer matches:
- `desugar.moduleDef` uses one `Ident(Foo$)` tree as the module val's type and in its `new
  Foo$()`; TASTy writes the second as a `SHAREDterm`. Our typer and reader make two nodes.
- `PostTyper` turns a reference to a class's self (`self =>`) into `This(cls)`, written as
  `QUALTHIS` with the class name; our typed tree keeps the `Ident`.

## Decision
The tree pickler (`crates/typer/src/pickle/`) writes these shapes itself: the class of a module
val's `new` as a reference to the val's type tree, a reference to a self as its class's `this`
with the span dotty gives the qualifier. The typed trees stay as dotty's typer gives them.

## Consequences
- `typer-compare` and the lowering are untouched; quotes read back from dotty's TASTy and
  pickled again give dotty's trees.
- A further PostTyper rewrite seen in quotes is added to the pickler the same way.
