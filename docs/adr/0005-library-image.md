# ADR 0005 — Library image and lazy decoding

Status: accepted · Date: 2026-09-27

## Context
Every run opened six library jars (24 ms for their zip directories), inflated each NIR file it
touched and decoded every definition in it (19 ms for the ~330 files hello touches). The
roadmap's startup target is a cached hello in ≤ 20 ms; it took 47 ms.

## Decision
- **Library image.** The CLI builds one file with every `.nir` file of the library jars,
  uncompressed, with a sorted name index (`crates/libimage/src/image.rs`), and maps it read-only
  for the rest of the process; files are read from the mapping without copying.
  It is keyed by the jars' canonical paths, sizes and modification times and lives in
  `$SCALAI_CACHE`, else the user's cache directory (`~/Library/Caches/scalai`,
  `$XDG_CACHE_HOME/scalai` or `~/.cache/scalai`). The first run builds it (≈0.4 s, 36 MB);
  if that fails, the jars are read directly. User classpath entries stay directories or jars.
- **Member tables.** Per file the image holds its members' signatures, sorted, with their
  definition offsets and which are fields (`crates/vm/src/member_table.rs`). A definition is
  found by binary search on its signature and a class's fields without looking at its other
  members, so a file's index is not decoded. Files holding members of other owners get no table
  and are indexed as before; none of Scala Native 0.5.12's 9,677 library files does.
- **Lazy decoding.** A definition decodes on first use, with the file's decode caches kept;
  files without a table read their index first. Local names and lexical scopes are skipped.

## Consequences
- Hello takes 20 ms and moderate 35 ms (from 47 and 70); opening the classpath costs 0.3 ms.
  With member tables (2026-09-28): hello 9.3 ms, moderate 17.4 ms.
- The image is a cache: deleting it is always safe; a changed jar produces a new file. Each
  use marks an image as used; building one removes images unused for 30 days
  (`crates/libimage/src/cache.rs`). The key also covers the program's version; a truncated
  image is rebuilt.
- The remaining startup cost is initializing the runtime's modules (~330 files, decoding and
  linking); a heap snapshot is the next step.
- 2026-09-30: the image also holds the TASTy the frontend reads (the standard library's,
  nativelib's, clib's, posixlib's, and javalib's, built into the binary); opening them takes
  0.3–1.3 ms instead of 17 ms through the jars. The image code lives in `crates/libimage`,
  shared by the VM and `crates/tasty`; the VM's member tables are its per-file extra blobs.
- 2026-10-14: the image also holds files derived from its file names, keyed by their names:
  the TASTy package table (`crates/tasty/src/symbols/packages.rs`), so that opening the
  frontend's universe reads one file instead of every name in the index (3.0 → 0.4 ms).
