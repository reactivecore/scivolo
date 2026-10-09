//! The top-level imports of an input as `:imports` lists them: dotty's
//! `RefinedPrinter.importText` of the typed import.

use syntax::names::Names;
use syntax::source::SourceFile;
use tasty::Universe;
use typer::Typed;
use typer::tpd::{TNodeId, Tree};

/// An input as `:imports` reads it: its typed wrapper template, names and source.
pub struct Line<'a> {
    pub typed: &'a Typed,
    pub template: TNodeId,
    pub names: &'a Names,
    pub source: &'a SourceFile,
}

/// The imports of `input`, as `:imports` lists them.
pub fn shown(u: &mut Universe, input: &Line) -> Vec<String> {
    let tpd = &input.typed.tpd;
    let Tree::Template { body, .. } = *tpd.tree(input.template) else { return Vec::new() };
    let mut out = Vec::new();
    for &s in tpd.items(body) {
        let Tree::Import { expr, selectors } = *tpd.tree(s) else { continue };
        let selectors: Vec<(String, bool)> =
            tpd.items(selectors).iter().map(|&s| selector(input, s)).collect();
        let selectors = match &selectors[..] {
            [(text, true)] => text.clone(),
            _ => format!("{{{}}}", selectors.into_iter().map(|(t, _)| t).collect::<Vec<_>>().join(", ")),
        };
        out.push(format!("import {}.{selectors}", path(u, input, expr)));
    }
    out
}

/// The source text of tree `t`.
fn text<'a>(input: &Line<'a>, t: TNodeId) -> &'a str {
    let span = input.typed.tpd.span(t);
    input.source.text.get(span.start() as usize..span.end() as usize).unwrap_or_default()
}

/// The text of import selector `s`, and whether it is a name or wildcard alone.
fn selector(input: &Line, s: TNodeId) -> (String, bool) {
    let tpd = &input.typed.tpd;
    let name = |t: TNodeId| match *tpd.tree(t) {
        Tree::Ident { name } => input.names.text(name.name),
        _ => text(input, t),
    };
    let Tree::ImportSelector { imported, renamed, bound } = *tpd.tree(s) else {
        return (text(input, s).into(), false);
    };
    let given = name(imported).is_empty();
    let mut out = match name(imported) {
        "_" => "*".to_owned(),
        "" => "given".to_owned(),
        "*" => "`*`".to_owned(),
        n => n.to_owned(),
    };
    if let Some(r) = renamed {
        out = format!("{out} as {}", name(r));
    }
    if let Some(b) = bound {
        out = format!("{out}{}{}", if given { " " } else { " : " }, text(input, b));
    }
    (out, renamed.is_none() && bound.is_none())
}

/// The text of import path `t`: a selection as written, an identifier as its symbol's path.
fn path(u: &mut Universe, input: &Line, t: TNodeId) -> String {
    let tpd = &input.typed.tpd;
    match *tpd.tree(t) {
        Tree::Select { qualifier, name } => {
            format!("{}.{}", path(u, input, qualifier), input.names.text(name.name))
        }
        Tree::Ident { name } if text(input, t).starts_with('`') => {
            format!("`{}`", input.names.text(name.name))
        }
        Tree::Ident { name } => match tpd.sym(t).map(|s| u.show_import_ident(s)) {
            Some(shown) if !shown.is_empty() => shown,
            _ => input.names.text(name.name).to_owned(),
        },
        _ => text(input, t).to_owned(),
    }
}
