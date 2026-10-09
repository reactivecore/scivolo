# ADR 0016 — The REPL shares Scala's history file

Status: accepted · Date: 2026-10-09

## Context
Scala's REPL keeps its input history in `~/.dotty_history`, through JLine's `DefaultHistory`.
Our REPL kept its own file in the cache, in rustyline's format. Users switching between both
REPLs expect one history.

## Decision
- The interactive REPL reads and appends `~/.dotty_history` in JLine's format and with
  JLine's rules and size limits as `scala` 3.9.0 configures them (`crates/scalai/src/history.rs`):
  entries `<epoch millis>:<line>` with `\n`, `\r`, `\` escaped; an input starting with a space
  is not added, others are trimmed and not added after an equal entry; a file grown too long is
  trimmed to its last distinct entries.
- rustyline keeps the entries in a `MemHistory` with its own filters off; its file history is
  not used.
- Differences to JLine, all to keep the user's file intact:
  - the file is read once per session, not before every prompt; entries of a concurrent
    session appear only in the next one;
  - a file with lines that are not entries is rewritten with all its valid lines verbatim
    (JLine writes back only the entries it holds in memory); bytes that are not UTF-8 are
    kept; a line whose text ends in an odd number of `\` is not an entry (JLine's REPL fails
    to start on it);
  - a file we cannot write is not rewritten;
  - rewrites go through a new temporary file beside the link target, with the original's
    permissions; an append after a last line without line end starts a new line;
  - an input of only whitespace is not added (JLine adds an empty entry).

## Consequences
- Entries typed in either REPL appear in both.
- Without a home directory the history lives in memory only.
- A session that appends while another rewrites the file (to trim it or drop bad lines) loses
  that entry from the file, as with JLine.
