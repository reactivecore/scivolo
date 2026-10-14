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
- **Keyed by the build.** The image tag also holds `SCALAI_BUILD`, the hash of the workspace's
  sources (`crates/scalai/build.rs`, also the build script of `crates/repl`, whose tests open
  images), so a changed program never reads tables, NIR member tables included, that older code
  wrote. Building an image removes images unused for a day.
- **Checked.** When a file entered from its table is decoded, each entered tree must exist and
  have the tag of its kind; otherwise decoding panics, naming the file.

## Consequences
- Measured (2026-10-14, edit-then-run, strict frontend, minima of 20): hello 18.8 → 17.8 ms,
  caseclasses 36.2 → 35.7 ms, a moderate variant 52.3 → 50.3 ms. Files decoded: hello 22 of 57
  loaded (30k of 73k nodes), caseclasses 52 of 75 (83k of 113k), moderate variant 151 of 183
  (200k of 241k): most loaded files still have a tree read through `info` or `parents`.
- Every change to the workspace's sources builds a new image on first use (about a second).
- Two readers of one format: `crates/tasty/tests/image.rs` builds an image afresh and checks that
  every symbol of the standard library, nativelib, clib, posixlib and javalib entered from tables
  equals the decoded one.
- Building the image decodes every TASTy file once more.
- A file decoded lazily panics if its bytes no longer decode; they are the bytes the table was
  built from, so only a corrupted image does.
