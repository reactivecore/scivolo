//! A source file through the whole frontend: parser, desugaring, typer, lowering.

use crate::NirFile;
use syntax::config::Config;
use syntax::desugar::Desugar;
use syntax::names::Names;
use syntax::parse::Parser;
use syntax::source::SourceFile;
use tasty::Universe;
use typer::Fallback;

/// The NIR files of `source`, or why it takes the JVM frontend.
pub fn compile(u: &mut Universe, source: &SourceFile, options: &[&str]) -> Result<Vec<NirFile>, String> {
    let mut names = Names::default();
    let mut p = Parser::new(source, &mut names, Config::from_options(options));
    let root = p.parse();
    let cfg = p.s.cfg;
    let root = Desugar::new(&mut p.ast, p.s.names, &mut p.s.diags, &mut p.fresh, source, cfg).unit(root);
    if p.s.diags.has_errors() {
        return Err("syntax errors".into());
    }
    let mut typed = match typer::type_unit(u, &mut p.ast, p.s.names, root) {
        Ok(typed) => typed,
        Err(Fallback::Unsupported { what, .. }) => return Err(format!("the typer does not support {what}")),
        Err(Fallback::Errors(_)) => return Err("type errors".into()),
    };
    crate::lower(u, &mut typed.tpd, typed.root, p.s.names, &mut p.fresh, source)
        .map_err(|e| format!("the lowering does not support {}", e.what))
}
