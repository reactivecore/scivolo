// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! Diagnostics of completions, reported even when the typing that triggered them is retracted
//! (dotty's `Completer` reporting in its creation context).

use super::Typer;
use syntax::diag::Diagnostic;

impl Typer<'_> {
    /// Records the diagnostics from `mark` on as reported by a completion.
    pub fn record_completion_diags(&mut self, mark: usize) {
        let reported = self.diags.items.get(mark..).unwrap_or_default();
        self.completion_diags.extend(reported.iter().cloned());
    }

    /// Drops the diagnostics from `mark` on but those completions reported.
    pub fn retract_diags(&mut self, mark: usize) {
        let dropped = self.diags.items.split_off(mark.min(self.diags.items.len()));
        let kept = dropped.into_iter().filter(|d| self.completion_diags.iter().any(|c| same(c, d)));
        self.diags.items.extend(kept);
    }
}

fn same(a: &Diagnostic, b: &Diagnostic) -> bool {
    (a.level, a.start, a.end, a.offset, &a.message) == (b.level, b.start, b.end, b.offset, &b.message)
}
