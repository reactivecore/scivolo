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
    let sources = Sources::new(files);
    let (mut names, mut fresh, mut ast) = (Names::default(), FreshNames::default(), Ast::default());
    let mut roots = Vec::new();
    for &(base, source) in sources.files() {
        let mark = ast.mark();
        let mut p = Parser::new(source, &mut names, cfg);
        (p.ast, p.fresh) = (std::mem::take(&mut ast), std::mem::take(&mut fresh));
        let root = p.parse();
        let file_cfg = p.s.cfg;
        let root =
            Desugar::new(&mut p.ast, p.s.names, &mut p.s.diags, &mut p.fresh, source, file_cfg).unit(root);
        if p.s.diags.has_errors() {
            let local = Sources::new([source]);
            return Err(refusal(&format!("syntax errors in {}", source.name), &p.s.diags, &local));
        }
        if file_cfg.source_version != cfg.source_version {
            let what = format!("{}: a source version of its own in a unit of several files", source.name);
            return Err(Refusal { what, diags: Diagnostics::default() });
        }
        (ast, fresh) = (p.ast, p.fresh);
        ast.shift_spans(mark, base);
        roots.push(root);
    }
    let typed = Units::default().type_units(u, &mut ast, (&mut names, &mut fresh), &roots, cfg);
    let mut typed = match typed {
        Ok(typed) => typed,
        Err(Fallback::Unsupported { what, at }) => {
            let what = format!("the typer does not support {what}{}", at_point(&sources, at));
            return Err(Refusal { what, diags: Diagnostics::default() });
        }
        Err(Fallback::Errors(diags)) => return Err(refusal("type errors", &diags, &sources)),
    };
    let files = crate::lower_sources(u, &mut typed.tpd, typed.root, (&mut names, &mut fresh), sources)
        .map_err(|e| {
            let what = format!("the lowering does not support {}", e.what);
            Refusal {
                what: format!("{what}{}", at_point(&Sources::new(files), e.at)),
                diags: Diagnostics::default(),
            }
        })?;
    Ok((files, typed.diags))
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
    Refusal { what: format!("{what}:{}", errors.collect::<String>()), diags: Diagnostics::default() }
}
