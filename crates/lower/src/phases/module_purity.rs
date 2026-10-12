//! The objects a unit refers to whose creation runs code: their class or one of its base classes
//! initializes something (dotty's not `isNoInitsRealClass`), judged before the phases change class
//! bodies.

use super::walk::subtrees;
use crate::Lower;
use rustc_hash::FxHashSet;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

/// Records in `l.impure_modules` the objects referred to in unit `root` that initialize something.
pub fn record(l: &mut Lower, root: TNodeId) {
    let mut seen = FxHashSet::default();
    for t in subtrees(l.tpd, root) {
        if !matches!(l.tpd.tree(t), Tree::Ident { .. } | Tree::Select { .. }) {
            continue;
        }
        let Some(s) = l.tpd.sym(t).filter(|&s| l.u.is_module(s) && seen.insert(s)) else { continue };
        if !is_no_inits_real_class(l, s) {
            l.impure_modules.insert(s);
        }
    }
}

/// Whether path `t` refers to or selects through an object whose creation runs code.
pub fn creates_impure_module(l: &Lower, t: TNodeId) -> bool {
    let impure = l.tpd.sym(t).is_some_and(|s| l.impure_modules.contains(&s));
    match *l.tpd.tree(t) {
        Tree::Ident { .. } => impure,
        Tree::Select { qualifier, .. } => impure || creates_impure_module(l, qualifier),
        _ => false,
    }
}

/// Whether the class of object `module` and all its base classes initialize nothing.
fn is_no_inits_real_class(l: &mut Lower, module: SymbolId) -> bool {
    let Some(class) = l.u.module_class(module) else { return true };
    l.u.linearization(class).into_iter().all(|b| l.u.no_inits(b))
}
