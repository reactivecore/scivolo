//! A source file through the whole frontend: parser, desugaring, typer, lowering.

use crate::NirFile;
use syntax::config::Config;
use syntax::desugar::Desugar;
use syntax::diag::Diagnostics;
use syntax::names::Names;
use syntax::parse::Parser;
use syntax::source::SourceFile;
use tasty::Universe;
use typer::Fallback;
use typer::inlining::Macros;

/// Why a source takes the JVM frontend, with the syntax or type errors behind it.
pub struct Refusal {
    pub what: String,
    pub diags: Diagnostics,
}

impl Refusal {
    fn new(what: String) -> Self {
        Refusal { what, diags: Diagnostics::default() }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.what)
    }
}

impl std::fmt::Debug for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.what)
    }
}

/// The NIR files of `source`, or why it takes the JVM frontend.
pub fn compile(u: &mut Universe, source: &SourceFile, options: &[&str]) -> Result<Vec<NirFile>, Refusal> {
    compile_with(u, source, Config::from_options(options), None).map(|(files, _)| files)
}

/// The NIR files and warnings of `source` under settings `cfg`, its macro calls expanded
/// through `macros`, or why it takes the JVM frontend.
pub fn compile_with(
    u: &mut Universe,
    source: &SourceFile,
    cfg: Config,
    macros: Option<&mut dyn Macros>,
) -> Result<(Vec<NirFile>, Diagnostics), Refusal> {
    let mut names = Names::default();
    let mut p = Parser::new(source, &mut names, cfg);
    let root = p.parse();
    let cfg = p.s.cfg;
    let root = Desugar::new(&mut p.ast, p.s.names, &mut p.s.diags, &mut p.fresh, source, cfg).unit(root);
    if p.s.diags.has_errors() {
        return Err(Refusal { what: "syntax errors".into(), diags: std::mem::take(&mut p.s.diags) });
    }
    let mut typed = match typer::type_unit(u, &mut p.ast, (p.s.names, &mut p.fresh), (root, cfg), macros) {
        Ok(typed) => typed,
        Err(Fallback::Unsupported { what, .. }) => {
            return Err(Refusal::new(format!("the typer does not support {what}")));
        }
        Err(Fallback::Errors(diags)) => return Err(Refusal { what: "type errors".into(), diags }),
    };
    let files =
        crate::lower(u, &mut typed.tpd, typed.root, p.s.names, &mut p.fresh, source).map_err(|e| {
            let at = if e.at.exists() {
                let line = source.offset_to_line(e.at.point());
                format!(" at {}:{}", line + 1, source.column(e.at.point()) + 1)
            } else {
                String::new()
            };
            Refusal::new(format!("the lowering does not support {}{at}", e.what))
        })?;
    Ok((files, typed.diags))
}
