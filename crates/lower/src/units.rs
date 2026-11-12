//! Several source files through the whole frontend as one unit, each referring to the others.

use crate::{NirFile, Refusal};
use syntax::ast::Ast;
use syntax::config::Config;
use syntax::desugar::Desugar;
use syntax::diag::{Diagnostics, Level};
use syntax::names::{FreshNames, Names};
use syntax::parse::Parser;
use syntax::source::{Offset, SourceFile, Sources};
use tasty::Universe;
use typer::inlining::Macros;
use typer::pickle::Pickled;
use typer::tpd::TNodeId;
use typer::{Fallback, SourceUnit};

/// The NIR files and warnings of `files` under settings `cfg`, each file's language imports
/// added for it, typed together so that each sees the definitions of all, their macro calls
/// expanded through `macros`, or why they take the JVM frontend; the warnings' offsets are those
/// of `Sources::new(files)`.
pub fn compile_sources(
    u: &mut Universe,
    files: &[SourceFile],
    cfg: Config,
    macros: Option<&mut dyn Macros>,
) -> Result<(Vec<NirFile>, Diagnostics), Refusal> {
    let generate = |l: &mut crate::Lower, root, _| crate::codegen::unit(l, root);
    compile_units(u, files, cfg, macros, (crate::Backend::Native, generate))
}

/// `compile_sources` with the phases for `backend` and `generate` in place of NIR generation;
/// on the JVM, `generate` gets the TASTy of the top-level classes, pickled before the inline
/// calls expand.
pub fn compile_units<R>(
    u: &mut Universe,
    files: &[SourceFile],
    cfg: Config,
    macros: Option<&mut dyn Macros>,
    (backend, generate): (
        crate::Backend,
        impl FnOnce(&mut crate::Lower, TNodeId, Vec<Pickled>) -> crate::Result<R>,
    ),
) -> Result<(R, Diagnostics), Refusal> {
    compile_attributed(u, files, cfg, macros, (backend, generate)).map_err(|mut r| r.swap_remove(0).refusal)
}

/// A refusal of a unit and the indices of the files it lies in, none if it lies in no file.
pub struct Attributed {
    pub refusal: Refusal,
    pub files: Vec<usize>,
}

/// `compile_units`, its refusals attributed to the files they lie in: one or more, the first
/// the one `compile_units` gives.
pub fn compile_attributed<R>(
    u: &mut Universe,
    files: &[SourceFile],
    cfg: Config,
    macros: Option<&mut dyn Macros>,
    (backend, generate): (
        crate::Backend,
        impl FnOnce(&mut crate::Lower, TNodeId, Vec<Pickled>) -> crate::Result<R>,
    ),
) -> Result<(R, Diagnostics), Vec<Attributed>> {
    let whole = |refusal| vec![Attributed { refusal, files: Vec::new() }];
    let sources = Sources::new(files).map_err(|e| whole(refused(e)))?;
    let (mut names, mut ast) = (Names::default(), Ast::default());
    let mut fresh = FreshNames::default();
    let (mut units, mut syntax_errors) = (Vec::new(), Vec::new());
    for (i, &(base, source)) in sources.files().iter().enumerate() {
        let mark = ast.mark();
        let mut p = Parser::new(source, &mut names, cfg);
        p.ast = std::mem::take(&mut ast);
        let root = p.parse();
        let file_cfg = p.s.cfg;
        let root =
            Desugar::new(&mut p.ast, p.s.names, &mut p.s.diags, &mut p.fresh, source, file_cfg).unit(root);
        if p.s.diags.has_errors() {
            let what = format!("syntax errors in {}", source.name);
            let refusal = refusal(&what, &p.s.diags, &Sources::one(source));
            syntax_errors.push(Attributed { refusal, files: vec![i] });
        }
        p.fresh.uniques.shift(base);
        fresh.include(i, &p.fresh);
        ast = p.ast;
        ast.shift_spans(mark, base);
        units.push(SourceUnit { root, start: base, cfg: file_cfg });
    }
    if !syntax_errors.is_empty() {
        return Err(syntax_errors);
    }
    let mut typer = crate::typer_units();
    typer.source_names = files.iter().map(|f| f.name.clone()).collect();
    if backend == crate::Backend::Jvm {
        let copies = sources.files().iter().map(|&(base, f)| (copy(f), base));
        typer.tasty_sources = Some(copies.collect());
    }
    let typed = typer.type_units(u, &mut ast, (&mut names, &mut fresh), &units, macros);
    let mut typed = match typed {
        Ok(typed) => typed,
        Err(Fallback::Unsupported { what, at }) => {
            let all = match typer.refused.is_empty() {
                true => vec![(at, what)],
                false => std::mem::take(&mut typer.refused),
            };
            let attributed = all.into_iter().map(|(at, what)| {
                let what = format!("the typer does not support {what}{}", at_point(&sources, at));
                Attributed { refusal: refused(what), files: file_at(&sources, at) }
            });
            return Err(attributed.collect());
        }
        Err(Fallback::Errors(diags)) => {
            let files = error_files(&diags, &sources);
            return Err(vec![Attributed { refusal: refusal("type errors", &diags, &sources), files }]);
        }
    };
    let located = Sources::new(files).map_err(|e| whole(refused(e)))?;
    let sources = (sources, cfg.explicit_nulls);
    let pickles = std::mem::take(&mut typed.pickles);
    let mut in_backend = false;
    let generate = |l: &mut crate::Lower, root| generate(l, root, pickles).inspect_err(|_| in_backend = true);
    let trees = (&mut typed.tpd, &mut typed.diags);
    let lowered =
        crate::lower_trees(u, trees, typed.root, (&mut names, &mut fresh), sources, (backend, generate));
    let out = lowered.map_err(|e| {
        let stage = match in_backend && backend == crate::Backend::Jvm {
            true => "the JVM backend",
            false => "the lowering",
        };
        let what = format!("{stage} does not support {}{}", e.what, at_point(&located, e.at));
        vec![Attributed { refusal: refused(what), files: file_at(&located, e.at) }]
    })?;
    Ok((out, typed.diags))
}

/// The indices of the files the errors among `diags` are in, none if one of them has no position.
fn error_files(diags: &Diagnostics, sources: &Sources) -> Vec<usize> {
    let errors = diags.items.iter().filter(|d| d.level == Level::Error);
    let Some(mut files) = errors.map(|d| d.offset.map(|o| sources.index(o))).collect::<Option<Vec<_>>>()
    else {
        return Vec::new();
    };
    files.sort_unstable();
    files.dedup();
    files
}

/// The index of the file the point of `at` is in, if it has one in `sources`.
fn file_at(sources: &Sources, at: syntax::ast::Span) -> Vec<usize> {
    match at.exists() && !at.is_in_other_source() {
        true => vec![sources.index(at.without_inlined_call().point())],
        false => Vec::new(),
    }
}

fn copy(f: &SourceFile) -> SourceFile {
    let mut copy = SourceFile::new(f.name.clone(), f.text.clone());
    copy.tasty_path.clone_from(&f.tasty_path);
    copy
}

fn refused(what: String) -> Refusal {
    Refusal { what, diags: Diagnostics::default() }
}

/// ` at FILE:LINE:COLUMN` of the point of `at`, if it has one in `sources`.
fn at_point(sources: &Sources, at: syntax::ast::Span) -> String {
    match at.exists() && !at.is_in_other_source() {
        true => format!(" at {}", position(sources, at.without_inlined_call().point())),
        false => String::new(),
    }
}

/// `FILE:LINE:COLUMN` of `offset`.
pub fn position(sources: &Sources, offset: Offset) -> String {
    let (file, at) = sources.locate(offset);
    format!("{}:{}:{}", file.name, file.offset_to_line(at) + 1, file.column(at) + 1)
}

/// A refusal `what` listing the errors among `diags`, each with its position in `sources`.
fn refusal(what: &str, diags: &Diagnostics, sources: &Sources) -> Refusal {
    let errors = diags.items.iter().filter(|d| d.level == syntax::diag::Level::Error).map(|d| {
        let at = d.offset.map_or_else(String::new, |o| format!("{}: ", position(sources, o)));
        format!("\n  {at}{}", d.message)
    });
    refused(format!("{what}:{}", errors.collect::<String>()))
}
