# ADR 0005 — Library image and lazy decoding

Status: accepted · Date: 2026-09-27

## Context
Every run opened six library jars (24 ms for their zip directories), inflated each NIR file it
touched and decoded every definition in it (19 ms for the ~330 files hello touches). The
roadmap's startup target is a cached hello in ≤ 20 ms; it took 47 ms.

## Decision
- **Library image.** The CLI builds one file with every `.nir` file of the library jars,
  uncompressed, with a sorted name index (`crates/vm/src/image.rs`), and maps it read-only.
  It is keyed by the jars' canonical paths, sizes and modification times and lives in
  `$SCALAI_CACHE`, else the user's cache directory (`~/Library/Caches/scalai`,
  `$XDG_CACHE_HOME/scalai` or `~/.cache/scalai`). The first run builds it (≈0.3 s, 29 MB);
  if that fails, the jars are read directly. User classpath entries stay directories or jars.
- **Lazy decoding.** Loading a file reads only its definition index; a definition decodes on
  first use, with the file's decode caches kept. Local names and lexical scopes are skipped.

## Consequences
- Hello takes 20 ms and moderate 35 ms (from 47 and 70); opening the classpath costs 0.3 ms.
- The image is a cache: deleting it is always safe; a changed jar produces a new file and old
  images stay behind until removed.
- The remaining startup cost is initializing the runtime's modules (~330 files, decoding and
  linking); a heap snapshot is the next step.
