//! dotty's `UninitializedDefs`: a variable of a class whose right-hand side is
//! `compiletime.uninitialized` gets `_` instead, a field without an initializer; and
//! `TransformWildcards` for such fields that become values of the constructor.

use super::casts::cast;
use super::literals::is_wildcard;
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use tasty::tags::MUTABLE;
use tasty::{Erased, SymbolId};
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

/// Refuses the references to `compiletime.uninitialized` under `root` that `transform` left.
pub fn refuse_others(l: &Lower, root: TNodeId) -> Result<()> {
    for t in super::walk::subtrees(l.tpd, root) {
        if l.tpd.sym(t).is_some_and(|s| is_uninitialized(l, s)) {
            return unsupported(l.tpd.span(t), "`uninitialized` other than as a variable field's value");
        }
    }
    Ok(())
}

/// Drops the `_` of field definition `stat` of `class`; the zero of its type instead if the
/// field became a value of the constructor.
pub fn drop_wildcard(l: &mut Lower, stat: TNodeId, class: SymbolId) {
    let Tree::ValDef { name, tpt, rhs: Some(rhs) } = *l.tpd.tree(stat) else { return };
    let Some(field) = l.tpd.sym(stat).filter(|_| is_wildcard(l, rhs)) else { return };
    let init = (l.u.symbol(field).owner != Some(class)).then(|| {
        let info = l.u.info(field);
        match l.u.erase_value(info) {
            erased @ (Erased::Class(_) | Erased::Array(_)) => {
                let null = l.null_literal(l.tpd.span(tpt).end_pos());
                let c = cast(l, null, &erased);
                l.null_casts.insert(c);
                c
            }
            erased => l.default_value(&erased, l.tpd.span(rhs)),
        }
    });
    l.tpd.get_mut(stat).tree = Tree::ValDef { name, tpt, rhs: init };
}

/// Whether `s` is `scala.compiletime.uninitialized`.
fn is_uninitialized(l: &Lower, s: SymbolId) -> bool {
    let sym = l.u.symbol(s);
    let package = sym.owner.and_then(|o| l.u.symbol(o).owner);
    sym.name == "uninitialized" && package.is_some() && package == l.u.package("scala.compiletime")
}
