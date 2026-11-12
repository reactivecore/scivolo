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
    /// synthetic members and inline calls. Each stage runs on the files no refusal lies in so far,
    /// so that the refusals of all files are collected, and not at all after type errors or a
    /// refusal in no file.
    pub fn finish(&mut self, mut typer: Typer, typed: TNodeId, files: &[TNodeId]) -> Result<Typed, Fallback> {
        let mut pickles = Vec::new();
        self.note(&mut typer);
        typer.typed_annotations();
        if !self.typer_only {
            typer.top_level_annotations(files, &self.source_names);
        }
        typer.check_dynamic_calls();
        if let Err(what) = typer.zonk_trees() {
            typer.unsupported_at(Span::NONE, what);
        }
        typer.renumber_uniques();
        self.note(&mut typer);
        if let Some(root) = self.kept_root(&mut typer, typed, files) {
            typer.check_variances(root);
            self.note(&mut typer);
        }
        if !self.typer_only {
            self.expand(&mut typer, typed, files, &mut pickles);
        }
        if let Some(root) = self.kept_root(&mut typer, typed, files) {
            typer.check_macro_annotated();
            typer.check_forward_refs(root);
        }
        if std::mem::take(&mut typer.u.reductions_exhausted) {
            typer.unsupported_at(Span::NONE, "match type reduction over the reduction budget");
        }
        self.note(&mut typer);
        self.syms = std::mem::take(&mut typer.syms);
        self.syms.end_unit(typer.u);
        if self.errors_first && typer.diags.has_errors() {
            return Err(Fallback::Errors(typer.diags));
        }
        if let Some((at, what)) = self.refused.first().cloned() {
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

    /// Flattens the spreads of the files of `files` not refused, adds their synthetic members,
    /// pickles them into `pickles` and expands their inline calls.
    fn expand(&mut self, typer: &mut Typer, typed: TNodeId, files: &[TNodeId], pickles: &mut Vec<Pickled>) {
        if let Some(root) = self.kept_root(typer, typed, files)
            && !typer.spread_ctxs.is_empty()
        {
            typer.flatten_spreads(root);
        }
        if let Some(root) = self.kept_root(typer, typed, files) {
            let lent = (&mut *typer.u, &mut typer.tpd, &mut *typer.names, &mut *typer.fresh);
            if let Err((at, what)) =
                self.synthetic_members.map_or(Ok(()), |add| add(lent, root, &typer.file_starts))
            {
                typer.unsupported_at(at, &what);
                self.note(typer);
            }
        }
        if let Some(kept) = self.kept_files(typer, files) {
            self.pickle_files(typer, &kept, pickles);
        }
        if let Some(root) = self.kept_root(typer, typed, files) {
            for (at, what) in typer.with_inlining(false, |i| i.run_all(root)) {
                self.refused.push((at, what.to_owned()));
            }
        }
    }

    /// Moves the typer's refusal into `refused`.
    fn note(&mut self, typer: &mut Typer) {
        self.refused.extend(typer.unsupported.take());
    }

    /// The top-level trees among `files` of the files no refusal lies in, none if there are type
    /// errors, a refusal in no file or no such file.
    fn kept_files(&self, typer: &Typer, files: &[TNodeId]) -> Option<Vec<TNodeId>> {
        if typer.diags.has_errors() {
            return None;
        }
        let in_file =
            |&(at, _): &(Span, String)| (at.exists() && !at.is_in_other_source()).then(|| typer.file_of(at));
        let refused = self.refused.iter().map(in_file).collect::<Option<Vec<usize>>>()?;
        let kept: Vec<TNodeId> =
            files.iter().copied().filter(|&f| !refused.contains(&typer.file_of(typer.tpd.span(f)))).collect();
        (!kept.is_empty()).then_some(kept)
    }

    /// `typed` if no refusal lies in its files `files`, else a package clause of the kept ones.
    fn kept_root(&self, typer: &mut Typer, typed: TNodeId, files: &[TNodeId]) -> Option<TNodeId> {
        let kept = self.kept_files(typer, files)?;
        Some(match kept.len() == files.len() {
            true => typed,
            false => root_package(typer, &kept),
        })
    }

    /// Pickles the top-level classes of typed source files `files` into `out` if `tasty_sources`
    /// is set; a refusal at each file refused.
    fn pickle_files(&mut self, typer: &mut Typer, files: &[TNodeId], out: &mut Vec<Pickled>) {
        let Some(sources) = &self.tasty_sources else { return };
        for &f in files {
            if let Err((at, what)) = pickle_one(typer, sources, f, out) {
                self.refused.push((at, format!("pickling {what} into TASTy")));
            }
        }
    }
}

/// Pickles the top-level classes of typed source file `f` of `sources` into `out`.
fn pickle_one(
    typer: &mut Typer,
    sources: &[(syntax::source::SourceFile, syntax::source::Offset)],
    f: TNodeId,
    out: &mut Vec<Pickled>,
) -> Result<(), (Span, Refusal)> {
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
    Ok(())
}
