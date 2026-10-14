//! The objects a unit refers to whose creation runs code: their class or one of its base classes
//! initializes something (dotty's not `isNoInitsRealClass`), judged before the phases change class
//! bodies.

use super::first_transform::{is_literal, is_pure_with_by_name};
use super::walk::subtrees;
use crate::Lower;
use crate::symbols::has;
use rustc_hash::FxHashSet;
use tasty::SymbolId;
use tasty::tags::LAZY;
use typer::tpd::{TNodeId, Tree};

/// The objects among class definitions `classes` with initialization code: a value with an
/// initializer, a statement or a parent constructor called with arguments (dotty's not
/// `NoInits`).
pub fn objects_with_inits(l: &mut Lower, classes: &[TNodeId]) -> FxHashSet<SymbolId> {
    let mut out = FxHashSet::default();
    for &td in classes {
        let Some(class) = l.tpd.sym(td).filter(|&c| l.u.is_module_class(c)) else { continue };
        let Tree::TypeDef { rhs, .. } = *l.tpd.tree(td) else { continue };
        let Tree::Template { body, parents, .. } = *l.tpd.tree(rhs) else { continue };
        let calls_with_args = |&p: &TNodeId| match *l.tpd.tree(p) {
            Tree::Apply { args, .. } => !l.tpd.items(args).is_empty(),
            Tree::Block { .. } => true,
            _ => false,
        };
        if l.tpd.items(parents).iter().any(calls_with_args) {
            out.extend(l.u.source_module(class));
            continue;
        }
        let initializes = |&s: &TNodeId| match *l.tpd.tree(s) {
            Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::Import { .. } => false,
            Tree::ValDef { rhs, .. } => {
                rhs.is_some() && l.tpd.sym(s).is_some_and(|v| !has(l.u, v, LAZY) && !l.u.is_module(v))
            }
            _ => true,
        };
        if l.tpd.items(body).iter().any(initializes) {
            out.extend(l.u.source_module(class));
        }
    }
    out
}

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

/// Whether evaluating `t` has no effect: a literal, or a pure path that creates no object whose
/// creation runs code.
pub fn is_pure_value(l: &Lower, t: TNodeId) -> bool {
    is_literal(l, t) || is_pure_with_by_name(l, t) && !creates_impure_module(l, t)
}

/// Whether path `t` refers to or selects through an object whose creation runs code.
fn creates_impure_module(l: &Lower, t: TNodeId) -> bool {
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
