// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/UncacheGivenAliases.scala
//! dotty's `UncacheGivenAliases`: a given alias whose right-hand side is `this`, `this.y` or `y`
//! of a stable `y` becomes a parameterless method.

use super::local_free::{enclosing_class, is_term};
use super::walk::subtrees;
use crate::Lower;
use crate::symbols::has;
use rustc_hash::FxHashSet;
use tasty::tags::{GIVEN, LAZY, MUTABLE, STABLE};
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, root: TNodeId) {
    let mut uncached = FxHashSet::default();
    for t in subtrees(l.tpd, root) {
        let (&Tree::ValDef { name, tpt, rhs: Some(rhs) }, Some(s)) = (l.tpd.tree(t), l.tpd.sym(t)) else {
            continue;
        };
        if !has(l.u, s, LAZY) || !has(l.u, s, GIVEN) || needs_cache(l, s, rhs, &uncached) {
            continue;
        }
        let flags = l.u.symbol(s).flags.without(LAZY);
        l.u.set_flags(s, flags);
        l.u.set_kind(s, Kind::Def);
        let paramss = l.tpd.clauses(&[]);
        l.tpd.get_mut(t).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) };
        uncached.insert(s);
    }
}

/// Whether given alias `s` must cache `rhs`: unless `rhs` is `this`, a stable local `y`, or a
/// stable member `y` or `this.y` of the class enclosing `s`.
fn needs_cache(l: &mut Lower, s: SymbolId, rhs: TNodeId, uncached: &FxHashSet<SymbolId>) -> bool {
    let owner = l.u.symbol(s).owner.expect("a given alias's owner");
    let class = enclosing_class(l, owner);
    let (y, prefix) = match *l.tpd.tree(rhs) {
        Tree::This { .. } => return false,
        Tree::Ident { .. } => {
            let Some(y) = l.tpd.sym(rhs) else { return true };
            let y_owner = l.u.symbol(y).owner.expect("a term's owner");
            let local = is_term(l, y_owner);
            (y, local || l.u.is_class(y_owner) && l.u.derives_from(class, y_owner))
        }
        Tree::Select { qualifier, .. } => {
            let this = matches!(l.tpd.tree(qualifier), Tree::This { .. });
            let Some(y) = l.tpd.sym(rhs).filter(|_| this) else { return true };
            (y, l.tpd.sym(qualifier) == Some(class))
        }
        _ => return true,
    };
    !prefix || !is_stable(l, y, uncached)
}

/// Whether `y` is an object, a value or parameter that is neither mutable nor by-name, or a
/// stable method.
fn is_stable(l: &mut Lower, y: SymbolId, uncached: &FxHashSet<SymbolId>) -> bool {
    match l.u.symbol(y).kind {
        _ if l.u.is_module(y) || uncached.contains(&y) => true,
        Kind::Val | Kind::Param => !has(l.u, y, MUTABLE) && !super::first_transform::is_by_name(l, y),
        Kind::Def => has(l.u, y, STABLE),
        _ => false,
    }
}
