//! What follows typing a unit: annotations, type variables, synthetic members, inline calls and
//! the checks after them.

use crate::Typer;
use crate::inlining::Lent;
use crate::pickle::{FileAttributes, Pickled, QuoteSource, Refusal, pickle_file};
use crate::tpd::TNodeId;
use crate::{Fallback, Typed, Units};
use syntax::ast::Span;

/// Adds the synthetic members of the classes of typed unit `root` (dotty's `SyntheticMembers` as
/// PostTyper runs it), or why it cannot.
pub type SyntheticMembers = fn(Lent<'_>, TNodeId) -> Result<(), (Span, String)>;

impl Units {
    /// Completes typed unit `typed` of source files `files`: its annotations, type variables,
    /// synthetic members and inline calls.
    pub fn finish(&mut self, mut typer: Typer, typed: TNodeId, files: &[TNodeId]) -> Result<Typed, Fallback> {
        let mut pickles = Vec::new();
        typer.typed_annotations();
        if !self.typer_only {
            typer.top_level_annotations(files, &self.source_names);
        }
        typer.check_dynamic_calls();
        if let Err(what) = typer.zonk_trees() {
            typer.unsupported_at(Span::NONE, what);
        }
        typer.renumber_uniques();
        if typer.unsupported.is_none() && !typer.diags.has_errors() {
            typer.check_variances(typed);
        }
        let clean = typer.unsupported.is_none() && !typer.diags.has_errors();
        if !self.typer_only && !typer.spread_ctxs.is_empty() && clean {
            typer.flatten_spreads(typed);
        }
        if !self.typer_only && typer.unsupported.is_none() && !typer.diags.has_errors() {
            let lent = (&mut *typer.u, &mut typer.tpd, &mut *typer.names, &mut *typer.fresh);
            if let Err((at, what)) = self.synthetic_members.map_or(Ok(()), |add| add(lent, typed)) {
                typer.unsupported_at(at, &what);
            } else if let Err(what) = self.pickle_files(&mut typer, files, &mut pickles) {
                typer.unsupported_at(Span::NONE, &format!("pickling {what} into TASTy"));
            } else if let Err((at, what)) = typer.with_inlining(false, |i| i.run(typed)) {
                typer.unsupported_at(at, what);
            }
        }
        if typer.unsupported.is_none() && !typer.diags.has_errors() {
            typer.check_macro_annotated();
            typer.check_forward_refs(typed);
        }
        if std::mem::take(&mut typer.u.reductions_exhausted) {
            typer.unsupported_at(Span::NONE, "match type reduction over the reduction budget");
        }
        self.syms = std::mem::take(&mut typer.syms);
        self.syms.end_unit(typer.u);
        if self.errors_first && typer.diags.has_errors() {
            return Err(Fallback::Errors(typer.diags));
        }
        if let Some((at, what)) = typer.unsupported.take() {
            return Err(Fallback::Unsupported { at, what });
        }
        if typer.diags.has_errors() {
            return Err(Fallback::Errors(typer.diags));
        }
        let mut diags = typer.diags;
        if !self.typer_only {
            diags.items.append(&mut typer.erasure_diags.items);
        }
        Ok(Typed { tpd: typer.tpd, root: typed, diags, pickles })
    }

    /// Pickles the top-level classes of typed source files `files` into `out` if `tasty_sources`
    /// is set.
    fn pickle_files(
        &self,
        typer: &mut Typer,
        files: &[TNodeId],
        out: &mut Vec<Pickled>,
    ) -> Result<(), Refusal> {
        let Some(sources) = &self.tasty_sources else { return Ok(()) };
        let attributes = FileAttributes { scala2: typer.scala2, explicit_nulls: typer.explicit_nulls };
        for &f in files {
            let Some((file, base)) = sources.get(typer.file_of(typer.tpd.span(f))) else {
                return Err("a file without its source");
            };
            let source = QuoteSource::of(file.tasty_path().to_owned(), file, *base);
            let cx = (&mut *typer.u, &mut typer.tpd, &*typer.names);
            out.extend(pickle_file(cx, f, &source, (attributes, &typer.syms.annotations))?);
        }
        Ok(())
    }
}
