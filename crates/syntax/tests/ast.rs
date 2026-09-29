//! Tree positions and printing, as dotty's `Positioned` and the oracle's printer.

use syntax::ast::constant::Constant;
use syntax::ast::flags;
use syntax::ast::modifiers::{Mod, ModKind, Modifiers};
use syntax::ast::print::Printer;
use syntax::ast::{Ast, Span, TName, Tree};
use syntax::names::Names;

fn show(ast: &Ast, names: &Names, id: syntax::ast::NodeId) -> String {
    let mut p = Printer { ast, names, out: String::new() };
    p.tree(id);
    p.out
}

#[test]
fn envelopes_cover_children_and_place_those_without_spans() {
    let (mut ast, mut names) = (Ast::default(), Names::default());
    let f = TName { name: names.intern("f"), is_type: false };
    let ident = ast.node(Tree::Ident { name: f });
    ast.at_span(ident, Span::new(0, 1, 0));
    let tt = ast.node(Tree::TypeTree);
    let args = ast.list(&[tt]);
    let app = ast.node(Tree::Apply { fun: ident, args });
    assert_eq!(show(&ast, &names, app), "(Apply 0..1 (Ident 0..1@0 t:f) [ (TypeTree 1..1)])");
    ast.at_span(app, Span::new(0, 3, 1));
    assert_eq!(ast.span(app), Span::new(0, 3, 1));
    ast.at_span(app, Span::new(0, 9, 0));
    assert_eq!(ast.span(app), Span::new(0, 3, 1), "a source-derived span stays");
}

#[test]
fn with_span_fills_children_from_the_start() {
    let (mut ast, names) = (Ast::default(), Names::default());
    let tt = ast.node(Tree::TypeTree);
    let lit = ast.node(Tree::Literal { value: Constant::Int(1) });
    let typed = ast.node(Tree::Typed { expr: lit, tpt: tt });
    assert!(!ast.span(typed).exists());
    ast.at_span(typed, Span::new(4, 8, 4));
    assert_eq!(show(&ast, &names, typed), "(Typed 4..8@4 (Literal 4..4 #Int:\"1\") (TypeTree 4..4))");
}

#[test]
fn definitions_print_their_modifiers() {
    let (mut ast, mut names) = (Ast::default(), Names::default());
    let x = TName { name: names.intern("x"), is_type: false };
    let tt = ast.node(Tree::TypeTree);
    let mods = Modifiers {
        flags: flags::LAZY.or(flags::PRIVATE_LOCAL),
        mods: vec![Mod { kind: ModKind::Lazy, span: Span::new(0, 4, 0) }],
        ..Modifiers::default()
    };
    let mods = ast.mods(mods);
    let val = ast.node(Tree::ValDef { name: x, tpt: tt, rhs: None, mods });
    ast.at_span(val, Span::new(0, 10, 9));
    assert_eq!(
        show(&ast, &names, val),
        "(ValDef 0..10@9 (Mods [private[this] lazy] [] [ (Lazy 0..4@0)]) t:x (TypeTree 0..0) _)"
    );
}
