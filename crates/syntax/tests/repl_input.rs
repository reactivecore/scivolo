//! REPL input parsed as block statements, and whether it is incomplete.

use syntax::ast::Tree;
use syntax::config::Config;
use syntax::names::Names;
use syntax::parse::Parser;
use syntax::source::SourceFile;

/// The number of statements, whether the input is incomplete, and whether it has errors.
fn parse(text: &str) -> (usize, bool, bool) {
    let source = SourceFile::maybe_incomplete("<repl>", text);
    let mut names = Names::default();
    let mut p = Parser::new(&source, &mut names, Config::default());
    let stats = p.parse_stats();
    (stats.len(), p.s.diags.is_incomplete(), p.s.diags.has_errors())
}

#[test]
fn input_ending_early_is_incomplete() {
    let texts = [
        "val x =",
        "val x = 1 +",
        "val x = 1 +\n",
        "val x:",
        "def f(x: Int) = {",
        "f(1,",
        "List(1, 2",
        "a(0) = ",
        "if x then",
        "if x then 1 else",
        "x match",
        "x match {\n  case 1 =>",
        "for x <- xs yield",
        "class A {",
        "trait T:",
        "object O:\n  val x = 1",
        "object O:\n  def f =\n    1",
        "def f =\n  val y = 1\n  y",
        "\"\"\"abc",
        "s\"\"\"abc ${x}",
        "/* abc",
        "/* a /* b */",
        "val x = <a>",
        "xs.map: x =>",
    ];
    for text in texts {
        let (_, incomplete, errors) = parse(text);
        assert!(incomplete && errors, "{text:?}");
    }
}

#[test]
fn complete_input_is_not_incomplete() {
    let texts = [
        "val x = 1",
        "1 + 2\n",
        "def f(x: Int) = { x }",
        "object O:\n  val x = 1\n",
        "s\"\"\"abc ${x}\"\"\"",
        "/* abc */ 1",
        "import scala.collection.mutable",
    ];
    for text in texts {
        let (_, incomplete, errors) = parse(text);
        assert!(!incomplete && !errors, "{text:?}");
    }
}

#[test]
fn unclosed_single_line_literal_is_an_error() {
    for text in ["\"abc", "'a", "s\"abc"] {
        let (_, incomplete, errors) = parse(text);
        assert!(!incomplete && errors, "{text:?}");
    }
}

#[test]
fn indented_region_closes_at_a_trailing_line_break() {
    assert_eq!(parse("object O:\n  val x = 1\n"), (1, false, false));
}

#[test]
fn syntax_error_is_not_incomplete() {
    let (_, incomplete, errors) = parse("val x = )");
    assert!(!incomplete && errors);
}

#[test]
fn empty_input_is_complete() {
    assert_eq!(parse(""), (0, false, false));
}

#[test]
fn expression_is_one_statement() {
    let source = SourceFile::maybe_incomplete("<repl>", "1 + 2");
    let mut names = Names::default();
    let mut p = Parser::new(&source, &mut names, Config::default());
    let stats = p.parse_stats();
    assert!(!p.s.diags.has_errors());
    assert!(matches!(stats[..], [s] if matches!(p.ast.tree(s), Tree::InfixOp { .. })));
}

#[test]
fn normal_source_closes_indented_regions_at_eof() {
    let source = SourceFile::new("t.scala", "object O:\n  val x = 1");
    let mut names = Names::default();
    let mut p = Parser::new(&source, &mut names, Config::default());
    p.parse_stats();
    assert!(!p.s.diags.has_errors());
}
