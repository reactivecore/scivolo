//! What follows typing a unit: annotations, type variables, synthetic members, inline calls and
//! the checks after them.

use crate::Typer;
use crate::inlining::Lent;
use crate::pickle::{FileAttributes, Pickled, QuoteSource, Refusal, pickle_file};
use crate::tpd::TNodeId;
use crate::{Fallback, Typed, Units};
use syntax::ast::Span;

/// Adds the synthetic members of the classes of typed unit `root`, its files starting at the
/// offsets given but the first (dotty's `SyntheticMembers` as PostTyper runs it), or why it cannot.
pub type SyntheticMembers = fn(Lent<'_>, TNodeId, &[syntax::source::Offset]) -> Result<(), (Span, String)>;

/// Wraps typed source files `files` in a package clause of the root package.
pub fn root_package(typer: &mut Typer, files: &[TNodeId]) -> TNodeId {
    let root = typer.u.root();
    let tpe = typer.u.intern_type(tasty::Type::Package(root));
    let name = syntax::ast::TName { name: typer.names.intern(names::nme::ROOT_PKG), is_type: false };
    let pid = typer.tpd.node(crate::tpd::Tree::Ident { name }, Span::NONE, Some(tpe), Some(root));
    let stats = typer.tpd.list(files);
    typer.tpd.node(crate::tpd::Tree::PackageDef { pid, stats }, Span::NONE, Some(tpe), None)
}

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
            let starts = &typer.file_starts;
            if let Err((at, what)) = self.synthetic_members.map_or(Ok(()), |add| add(lent, typed, starts)) {
                typer.unsupported_at(at, &what);
            } else if let Err((at, what)) = self.pickle_files(&mut typer, files, &mut pickles) {
                typer.unsupported_at(at, &format!("pickling {what} into TASTy"));
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
    /// is set; a refusal at the file refused.
    fn pickle_files(
        &self,
        typer: &mut Typer,
        files: &[TNodeId],
        out: &mut Vec<Pickled>,
    ) -> Result<(), (Span, Refusal)> {
        let Some(sources) = &self.tasty_sources else { return Ok(()) };
        for &f in files {
            let at = typer.tpd.span(f);
            let index = typer.file_of(at);
            let Some((file, base)) = sources.get(index) else {
                return Err((at, "a file without its source"));
            };
            let scala2 = typer.file_cfgs.get(index).map_or(typer.scala2, |c| c.source_version.is_scala2());
            let attributes = FileAttributes { scala2, explicit_nulls: typer.explicit_nulls };
            let mut source = QuoteSource::of(file.tasty_path().to_owned(), file, *base);
            source.others = sources.iter().filter(|(_, b)| b != base).map(|(f, b)| (f, *b)).collect();
            let annotations = typer.with_child_annotations(f).map_err(|r| (at, r))?;
            let cx = (&mut *typer.u, &mut typer.tpd, &*typer.names);
            out.extend(pickle_file(cx, f, &source, (attributes, &annotations)).map_err(|r| (at, r))?);
        }
        Ok(())
    }
}
