# ADR 0016 — The REPL shares Scala's history file

Status: accepted · Date: 2026-10-09

## Context
Scala's REPL keeps its input history in `~/.dotty_history`, through JLine's `DefaultHistory`.
Our REPL kept its own file in the cache, in rustyline's format. Users switching between both
REPLs expect one history.

## Decision
- The interactive REPL reads and appends `$HOME/.dotty_history` in JLine's format and with
  JLine's rules as `scala` 3.9.0 configures them (`crates/scalai/src/history.rs`): entries
  `<epoch millis>:<line>` with `\n`, `\r`, `\` escaped; an input starting with a space is not
  added, others are trimmed and not added after an equal entry; 500 entries in memory; the
  file is trimmed to its last 10000 distinct entries once it holds more than 12500 lines.
- rustyline keeps the entries in a `MemHistory` with its own filters off; its file history is
  not used.
- Differences to JLine, all to keep the user's file intact:
  - the file is read once per session, not before every prompt; entries of a concurrent
    session appear only in the next one;
  - a file with lines that are not entries is rewritten with all its valid lines verbatim
    (JLine writes back only the 500 it holds in memory); bytes that are not UTF-8 are kept;
  - rewrites go through a temporary file beside the link target, with the original's
    permissions; an append after a last line without line end starts a new line;
  - an input of only whitespace is not added (JLine adds an empty entry).

## Consequences
- Entries typed in either REPL appear in both.
- Without `HOME` the history lives in memory only. Two sessions that rewrite the file at the
  same time can lose each other's newest entries, as with JLine.
