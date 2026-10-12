//! dotty's `PureStats` for anonymous instances: a statement `{ class C; new C(args) }` of a block,
//! `C` initializing nothing and `args` pure, is dropped.

use super::first_transform::is_pure;
use super::module_purity::creates_impure_module;
use super::walk::subtrees;
use crate::Lower;
use crate::symbols::has;
use tasty::tags::STABLE;
use typer::tpd::{TNodeId, Tree};

/// Drops the pure statements creating anonymous instances in unit `root`; the class definitions
/// dropped with them.
pub fn transform(l: &mut Lower, root: TNodeId) -> Vec<TNodeId> {
    let mut dropped = Vec::new();
    for t in subtrees(l.tpd, root) {
        let Tree::Block { stats, expr } = *l.tpd.tree(t) else { continue };
        let items = l.tpd.items(stats).to_vec();
        let (pure, kept): (Vec<TNodeId>, Vec<TNodeId>) = items.iter().partition(|&&s| is_pure_instance(l, s));
        if pure.is_empty() {
            continue;
        }
        for s in pure {
            let Tree::Block { stats, .. } = *l.tpd.tree(s) else { unreachable!("a block") };
            dropped.extend(l.tpd.items(stats));
        }
        let stats = l.tpd.list(&kept);
        l.tpd.get_mut(t).tree = Tree::Block { stats, expr };
    }
    dropped
}

/// Whether `t` is `{ class C; new C(args) }` with a stable constructor and pure arguments.
fn is_pure_instance(l: &Lower, t: TNodeId) -> bool {
    let Tree::Block { stats, expr } = *l.tpd.tree(t) else { return false };
    let defs = l.tpd.items(stats);
    if defs.is_empty() || !defs.iter().all(|&d| matches!(l.tpd.tree(d), Tree::TypeDef { .. })) {
        return false;
    }
    let Tree::Apply { fun, args } = *l.tpd.tree(l.tpd.strip_typed(expr)) else { return false };
    let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { return false };
    let stable = l.tpd.sym(fun).is_some_and(|c| l.u.is_constructor(c) && has(l.u, c, STABLE));
    let pure_arg = |&a: &TNodeId| {
        matches!(l.tpd.tree(a), Tree::Literal { .. }) || is_pure(l, a) && !creates_impure_module(l, a)
    };
    matches!(l.tpd.tree(qualifier), Tree::New { .. }) && stable && l.tpd.items(args).iter().all(pure_arg)
}
