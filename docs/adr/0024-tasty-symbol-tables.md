# ADR 0024 — TASTy symbol tables in the library image

Status: accepted · Date: 2026-10-14

## Context
Loading a TASTy file to enter its symbols decoded the whole file: the ASTs section into nodes and
every name as text (moderate: 188 files, 2.1 MB, 245k nodes; hello decodes `Character.tasty`,
136 KB). Most files are loaded for one member lookup and many of their trees are never read.

## Decision
- **Symbol table per TASTy file.** The library image keeps, as each TASTy file's extra blob
  (as ADR 0005 does for NIR member tables), the file's package clauses and definitions in
  preorder: name, kind, flags, owner entry, tree, `@targetName`, `@extern`, derived name, and
  whether the file is of the Scala 2 standard library (`crates/tasty/src/symbols/table.rs`).
- **One walk, two encodings.** Full decoding and the table come from the same function
  (`symbols/entries.rs`); a file is entered from its table when it has one, else decoded.
- **Lazy trees.** A file entered from its table decodes its trees and names on first use
  (`info`, `parents`, `body`, `file`), from the image's bytes.
- **Versioned.** The table starts with `SYMBOL_TABLE_VERSION`, which the image tag also holds; it
  changes with the layout or with what `entries` computes. A table of another version or a
  malformed one is ignored and the file decoded.

## Consequences
- Two readers of one format: `crates/tasty/tests/image.rs` builds an image afresh and checks that
  every symbol of the standard library, nativelib, clib, posixlib and javalib entered from tables
  equals the decoded one.
- Building the image decodes every TASTy file once more.
- A file decoded lazily panics if its bytes no longer decode; they are the bytes the table was
  built from, so only a corrupted image does.
