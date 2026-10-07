# scala_interpreter — project rules

Research project: a fast Scala 3 (3.9) interpreter for scripting, in Rust.
Research lives in `docs/research/`, throwaway experiments in `spikes/`, the
plan in `ROADMAP.md`. The four goals, in order of precedence when they
conflict: robustness > extensibility (the code can be extended easily) >
conciseness > performance. All four matter; performance comes last in a
conflict but is still a main goal.

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
  access by address goes through `heap::read`/`heap::write`), `stack/` (frame
  stacks and green-thread coroutines), and `natives/ffi/` and
  `natives/callback.rs` (the C boundary); outside it, only in
  `crates/libimage/src/image.rs` (maps the library image). The callback
  slot table (ADR 0002), the collector (ADR 0004) and the SIGINT counter
  (ADR 0013) are the only state outside `Vm`.
- Documentation is short: one home per fact.
- Code or ideas taken from elsewhere get an entry in `acknowledges.md`.

## Tooling

Create and change files with the Write/Edit tools, never with shell
redirection, heredocs or `sed -i`; read them with the Read tool, never with
`cat`, `head`, `tail` or `sed -n`. Those tools are pre-approved, the shell
forms are not. The shell already starts in the repository root:
never `cd`, not even as the first part of a compound command (`cd dir; grep …`
needs approval). Name files by their path from the root instead, e.g.
`grep -n "def genMethodSig" ref/scala-native/nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenType.scala`.
This applies to subagents too. Scratch files, dumps and experiments go to the gitignored `tmp/`
in this repo, reference checkouts to `ref/`; never `/tmp` or other
directories outside the repo.

## Commits

Very short messages: one summary line; body only when essential. No
Claude/Anthropic attribution trailers.

## GitLab

Work is tracked in GitLab issues (`reactivecore/scala-interpreter`, `glab`),
with the labels of rcui.

Issues and comments:

- Texts stay short.
- Numbers and bullets rather than paragraphs.

Merge requests:

- Review your own code before raising one.
- The title starts with the issue number (`#123`).
- The description summarizes; it does not repeat the issue text.
- A short note on *how* something was done belongs in it.
- It carries the checkboxes `Human Code Review` and `Human Test`, which are
  not exclusive:

  ```markdown
  - [ ] Human Code Review
  - [ ] Human Test
  ```
