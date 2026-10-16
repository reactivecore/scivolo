//! Several source files through the whole frontend as one unit, each referring to the others.

use crate::{NirFile, Refusal};
use syntax::ast::Ast;
use syntax::config::Config;
use syntax::desugar::Desugar;
use syntax::diag::Diagnostics;
use syntax::names::{FreshNames, Names};
use syntax::parse::Parser;
use syntax::source::{Offset, SourceFile, Sources};
use tasty::Universe;
use typer::{Fallback, Units};

/// The NIR files and warnings of `files` under settings `cfg`, typed together so that each sees
/// the definitions of all, or why they take the JVM frontend; the warnings' offsets are those of
/// `Sources::new(files)`.
pub fn compile_sources(
    u: &mut Universe,
    files: &[SourceFile],
    cfg: Config,
) -> Result<(Vec<NirFile>, Diagnostics), Refusal> {
    let sources = Sources::new(files).map_err(refused)?;
    let (mut names, mut ast) = (Names::default(), Ast::default());
    // Temporary until #115 (fresh names per file): the typer and the lowering count on from the
    // highest counters of the files' parsers.
    let mut fresh = FreshNames::default();
    let mut units = Vec::new();
    for &(base, source) in sources.files() {
        let mark = ast.mark();
        let mut p = Parser::new(source, &mut names, cfg);
        p.ast = std::mem::take(&mut ast);
        let root = p.parse();
        let file_cfg = p.s.cfg;
        let root =
            Desugar::new(&mut p.ast, p.s.names, &mut p.s.diags, &mut p.fresh, source, file_cfg).unit(root);
        if p.s.diags.has_errors() {
            let what = format!("syntax errors in {}", source.name);
            return Err(refusal(&what, &p.s.diags, &Sources::one(source)));
        }
        // Temporary until #115 (settings per file): a unit is typed with one `Config`.
        if file_cfg != cfg {
            return Err(refused(format!("{}: language imports in a unit of several files", source.name)));
        }
        fresh.include(&p.fresh);
        ast = p.ast;
        ast.shift_spans(mark, base);
        units.push((base, root));
    }
    let typed = Units::default().type_units(u, &mut ast, (&mut names, &mut fresh), &units, cfg);
    let mut typed = match typed {
        Ok(typed) => typed,
        Err(Fallback::Unsupported { what, at }) => {
            return Err(refused(format!("the typer does not support {what}{}", at_point(&sources, at))));
        }
        Err(Fallback::Errors(diags)) => return Err(refusal("type errors", &diags, &sources)),
    };
    let located = Sources::new(files).map_err(refused)?;
    let files = crate::lower_sources(u, &mut typed.tpd, typed.root, (&mut names, &mut fresh), sources)
        .map_err(|e| {
            refused(format!("the lowering does not support {}{}", e.what, at_point(&located, e.at)))
        })?;
    Ok((files, typed.diags))
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
