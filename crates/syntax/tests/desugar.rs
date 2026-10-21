//! Desugaring on small inputs in `tests/desugar`, against dotty 3.9.0's output recorded next
//! to each in a `.check` file (`just desugar-checks`); `just syntax-report desugar` compares it
//! on dotty's test suites.

use std::fmt::Write as _;
use std::path::Path;
use syntax::ast::print::{Printer, quote};
use syntax::config::Config;
use syntax::desugar::Desugar;
use syntax::diag::Level;
use syntax::names::{Names, to_utf16};
use syntax::parse::Parser;
use syntax::source::SourceFile;

/// The desugared tree of the file at `path` and its diagnostics, as `scivolo-syntax-dump desugar`
/// prints them.
fn desugar(path: &Path) -> String {
    let text = std::fs::read_to_string(path).expect("a test input");
    let options: Vec<&str> = text
        .lines()
        .filter_map(|l| l.strip_prefix("//> using options "))
        .flat_map(str::split_whitespace)
        .collect();
    let source = SourceFile::new(path.to_string_lossy(), text.as_str());
    let mut names = Names::default();
    let mut p = Parser::new(&source, &mut names, Config::from_options(&options));
    let root = p.parse();
    let cfg = p.s.cfg;
    let root = Desugar::new(&mut p.ast, p.s.names, &mut p.s.diags, &mut p.fresh, &source, cfg).unit(root);
    let mut printer = Printer { ast: &p.ast, names: p.s.names, out: String::new() };
    printer.tree(root);
    let mut out = printer.out;
    out.push('\n');
    for d in p.s.diags.unique() {
        let level = if d.level == Level::Error { "E" } else { "W" };
        writeln!(out, "== {level} {} {}", d.pos(), quote(&to_utf16(&d.message))).unwrap();
    }
    out
}

#[test]
fn desugared_trees_match_dotty() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/desugar");
    let mut inputs: Vec<_> = std::fs::read_dir(&dir)
        .expect("tests/desugar")
        .map(|e| e.expect("an entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "scala"))
        .collect();
    inputs.sort();
    assert!(!inputs.is_empty());
    for input in inputs {
        let expected = std::fs::read_to_string(input.with_extension("check")).expect("a .check file");
        assert_eq!(desugar(&input), expected, "{}", input.display());
    }
}
