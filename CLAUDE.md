# scala_interpreter — project rules

Research project: a fast Scala 3 (3.9) interpreter for scripting, in Rust.
Research lives in `docs/research/`, throwaway experiments in `spikes/`, the
plan in `ROADMAP.md`. Priorities in order: robustness > scalability >
conciseness > performance.

Rust style follows `/Users/nos/working/rc/rcui` (its `CLAUDE.md` is the
reference); the rules below are the ones that carry over.

## Build / test / lint

Once a workspace exists, every command lives in a `Justfile`; `just gates`
(fmt + clippy with `-D warnings` + test) must pass before every commit.
Spikes are exempt from the gates but still formatted with `rustfmt.toml`.

## Comments

Minimal. State only what code does, never why — no rationale, history,
alternatives, or reviewer notes. Every `unsafe` block carries a concise
`// SAFETY:` justification (mandatory). A module comment (`//!`) only where
the file name and items do not already say what it holds, a line or two.
Rationale goes in `docs/adr/`, one decision per file.

## Code style

- No source file over 200 lines, tests excluded; split by what the code is
  for, not by size. Exempt: a data table, a set of type definitions, or the
  API around a single type (e.g. the TASTy tag table).
- Group by meaning, not by syntactic form: no `types.rs`, `enums.rs`,
  `utils.rs`. A type lives beside the code that uses it.
- The reader is a Rust developer; use the shorter idiom where the language
  has one (`let … else`, `?`, iterator adapters).
- An item is `pub` or private; modules hide it. Only `mod` declarations and
  `macro_rules!` re-exports carry `pub(crate)`/`pub(super)`.
- No `Rc<RefCell<_>>` in runtime state, no global state, no async, no macros
  that obscure control flow, minimal generics. Arena / index handles over
  pointers. In the VM, `unsafe` lives only in `heap.rs` (every raw memory
  access by address goes through `heap::read`/`heap::write`), `stack.rs`, and
  `natives/ffi/` and `natives/callback.rs` (the C boundary); the callback
  slot table (ADR 0002) and the collector (ADR 0004) are the only state
  outside `Vm`.
- Documentation is short: one home per fact.
- Code or ideas taken from elsewhere get an entry in `acknowledges.md`.

## Tooling

Create and change files with the Write/Edit tools, never with shell
redirection, heredocs or `sed -i`; those are pre-approved, the shell forms
are not. Scratch files, dumps and experiments go to the gitignored `tmp/`
in this repo, reference checkouts to `ref/`; never `/tmp` or other
directories outside the repo.

## Commits

Very short messages: one summary line; body only when essential. No
Claude/Anthropic attribution trailers.
