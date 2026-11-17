//! Several source files through the whole frontend as one unit, each referring to the others.

use crate::unit_lowering::{lowered, lowered_kept};
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
    let first = |mut r: Vec<Attributed>| r.swap_remove(0).refusal;
    compile_attributed(u, files, (cfg, false), macros, (backend, generate)).map_err(first)
}

/// A refusal of a unit and the indices of the files it lies in, none if it lies in no file.
pub struct Attributed {
    pub refusal: Refusal,
    pub files: Vec<usize>,
}

/// `compile_units`, its refusals attributed to the files they lie in: one or more, the first
/// the one `compile_units` gives; with `all`, also the lowering's of the files the typer kept
/// when it refused others.
pub fn compile_attributed<R>(
    u: &mut Universe,
    files: &[SourceFile],
    (cfg, all): (Config, bool),
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
    let cfgs: Vec<Config> = match units.is_empty() {
        true => vec![cfg],
        false => units.iter().map(|unit| unit.cfg).collect(),
    };
    let unit = (files, cfgs.as_slice());
    let mut attributed = match typed {
        Ok(typed) => {
            return lowered(u, typed, (&mut names, &mut fresh), unit, (backend, generate))
                .map_err(|a| vec![a]);
        }
        Err(Fallback::Unsupported { what, at }) => {
            let all = match typer.refused.is_empty() {
                true => vec![(at, what)],
                false => std::mem::take(&mut typer.refused),
            };
            let attributed = all.into_iter().map(|(at, what)| {
                let what = format!("the typer does not support {what}{}", at_point(&sources, at));
                Attributed { refusal: refused(what), files: file_at(&sources, at) }
            });
            let mut attributed: Vec<Attributed> = attributed.collect();
            attributed.extend(type_errors(&std::mem::take(&mut typer.errors), &sources));
            attributed
        }
        Err(Fallback::Errors(diags)) => type_errors(&diags, &sources),
    };
    if let Some(kept) = typer.kept.take().filter(|_| all) {
        attributed.extend(lowered_kept(u, kept, (&mut names, &mut fresh), unit, (backend, generate)));
    }
    Err(attributed)
}

/// The refusal for the errors among `diags`, if any, in the files they are in, none if one of them
/// has no position.
fn type_errors(diags: &Diagnostics, sources: &Sources) -> Vec<Attributed> {
    if !diags.has_errors() {
        return Vec::new();
    }
    let errors = diags.items.iter().filter(|d| d.level == Level::Error);
    let mut files = errors.map(|d| d.offset.map(|o| sources.index(o))).collect::<Option<Vec<_>>>();
    if let Some(files) = &mut files {
        files.sort_unstable();
        files.dedup();
    }
    let files = files.unwrap_or_default();
    vec![Attributed { refusal: refusal("type errors", diags, sources), files }]
}

/// The index of the file the point of `at` is in, if it has one in `sources`.
pub fn file_at(sources: &Sources, at: syntax::ast::Span) -> Vec<usize> {
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

/// Makes refusal `what` without diagnostics.
pub fn refused(what: String) -> Refusal {
    Refusal { what, diags: Diagnostics::default() }
}

/// ` at FILE:LINE:COLUMN` of the point of `at`, if it has one in `sources`.
pub fn at_point(sources: &Sources, at: syntax::ast::Span) -> String {
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
