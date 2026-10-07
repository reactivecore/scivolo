//! A source file typed and printed in the format of `scripts/typer-oracle`: the typed tree and
//! the diagnostics, or why the file falls back.

use crate::Fallback;
use crate::tpd::print::Printer;
use std::fmt::Write;
use std::path::PathBuf;
use syntax::ast::print::quote;
use syntax::config::Config;
use syntax::desugar::Desugar;
use syntax::diag::{Diagnostics, Level};
use syntax::names::{Names, to_utf16};
use syntax::parse::Parser;
use syntax::source::SourceFile;
use tasty::Universe;

/// The library jars in `libs`, and javalib's TASTy checked in beside the crate.
pub fn library_jars(libs: &std::path::Path) -> Vec<PathBuf> {
    let jars = [
        "scala-library-3.9.0.jar",
        "nativelib_native0.5_3-0.5.12.jar",
        "clib_native0.5_3-0.5.12.jar",
        "posixlib_native0.5_3-0.5.12.jar",
    ];
    let mut out: Vec<PathBuf> = jars.iter().map(|j| libs.join(j)).collect();
    out.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../libs/javalib-tasty-0.5.12.jar"));
    out
}

pub fn dump(u: &mut Universe, source: &SourceFile, options: &[&str]) -> String {
    let mut names = Names::default();
    let mut p = Parser::new(source, &mut names, Config::from_options(options));
    let root = p.parse();
    let cfg = p.s.cfg;
    let root = Desugar::new(&mut p.ast, p.s.names, &mut p.s.diags, &mut p.fresh, source, cfg).unit(root);
    if p.s.diags.has_errors() {
        return "== syntax errors\n".into();
    }
    let mut b = String::new();
    match crate::type_unit(u, &mut p.ast, (p.s.names, &mut p.fresh), root, cfg.features) {
        Ok(typed) => {
            let mut printer = Printer { tpd: &typed.tpd, names: p.s.names, u, out: String::new() };
            printer.tree(typed.root);
            b.push_str(&printer.out);
            b.push('\n');
            diagnostics(&p.s.diags, &mut b);
            diagnostics(&typed.diags, &mut b);
        }
        Err(Fallback::Unsupported { at, what }) => {
            let at = if at.exists() { at.point().to_string() } else { "-".into() };
            writeln!(b, "== unsupported {what} @ {at}").unwrap();
        }
        Err(Fallback::Errors(diags)) => {
            b.push_str("_\n");
            diagnostics(&diags, &mut b);
        }
    }
    b
}

fn diagnostics(diags: &Diagnostics, b: &mut String) {
    for d in diags.unique() {
        let kind = if d.level == Level::Error { "E" } else { "W" };
        writeln!(b, "== {kind} {} {}", d.pos(), quote(&to_utf16(&d.message))).unwrap();
    }
}
