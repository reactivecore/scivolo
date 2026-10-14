//! dotty's `UninitializedDefs`: a variable of a class whose right-hand side is
//! `compiletime.uninitialized` gets `_` instead, a field without an initializer.

use crate::Lower;
use crate::symbols::has;
use tasty::SymbolId;
use tasty::tags::MUTABLE;
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, td: TNodeId) {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { return };
    let Tree::Template { body, .. } = *l.tpd.tree(template) else { return };
    for stat in l.tpd.items(body).to_vec() {
        let Tree::ValDef { name, tpt, rhs: Some(rhs) } = *l.tpd.tree(stat) else { continue };
        let mutable = l.tpd.sym(stat).is_some_and(|s| has(l.u, s, MUTABLE));
        if !mutable || !l.tpd.sym(rhs).is_some_and(|s| is_uninitialized(l, s)) {
            continue;
        }
        let tpe = l.tpd.tpe(tpt);
        let wildcard = l.wildcard(tpe);
        l.tpd.get_mut(wildcard).span = l.tpd.span(rhs);
        l.tpd.get_mut(stat).tree = Tree::ValDef { name, tpt, rhs: Some(wildcard) };
    }
}

/// Whether `t` is the `_` of a field without an initializer.
pub fn is_wildcard(l: &Lower, t: TNodeId) -> bool {
    matches!(*l.tpd.tree(t), Tree::Ident { name } if l.names.text(name.name) == "_")
}

/// Drops the `_` of field definition `stat` of `class`.
pub fn drop_wildcard(l: &mut Lower, stat: TNodeId, class: SymbolId) {
    let Tree::ValDef { name, tpt, rhs: Some(rhs) } = *l.tpd.tree(stat) else { return };
    let owner = l.tpd.sym(stat).and_then(|f| l.u.symbol(f).owner);
    if is_wildcard(l, rhs) && owner == Some(class) {
        l.tpd.get_mut(stat).tree = Tree::ValDef { name, tpt, rhs: None };
    }
}

/// Whether `s` is `scala.compiletime.uninitialized`.
fn is_uninitialized(l: &Lower, s: SymbolId) -> bool {
    let sym = l.u.symbol(s);
    let package = sym.owner.and_then(|o| l.u.symbol(o).owner);
    sym.name == "uninitialized" && package.is_some() && package == l.u.package("scala.compiletime")
}
