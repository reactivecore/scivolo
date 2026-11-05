// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PureStats.scala
//! dotty's `PureStats` for anonymous instances and unit blocks: a statement `{ class C; new
//! C(args) }` of a block, `C` initializing nothing and `args` pure, is dropped, and `{ paths; () }`
//! is replaced by its paths.

use super::module_purity::is_pure_value;
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

/// Replaces each statement `{ paths; () }` of the blocks in erased class `td` by its paths.
pub fn unit_blocks(l: &mut Lower, td: TNodeId) {
    for t in subtrees(l.tpd, td) {
        let Tree::Block { stats, expr } = *l.tpd.tree(t) else { continue };
        let items = l.tpd.items(stats).to_vec();
        if !items.iter().any(|&s| unit_block_paths(l, s).is_some()) {
            continue;
        }
        let flat: Vec<TNodeId> =
            items.iter().flat_map(|&s| unit_block_paths(l, s).unwrap_or_else(|| vec![s])).collect();
        let stats = l.tpd.list(&flat);
        l.tpd.get_mut(t).tree = Tree::Block { stats, expr };
    }
}

/// The paths of `t` if it is `{ paths; () }`, paths before a literal.
fn unit_block_paths(l: &Lower, t: TNodeId) -> Option<Vec<TNodeId>> {
    let Tree::Block { stats, expr } = *l.tpd.tree(t) else { return None };
    let paths = l.tpd.items(stats);
    let path =
        |s: &TNodeId| matches!(l.tpd.tree(*s), Tree::Ident { .. } | Tree::Select { .. } | Tree::This { .. });
    let literal = matches!(l.tpd.tree(expr), Tree::Literal { .. });
    (!paths.is_empty() && paths.iter().all(path) && literal).then(|| paths.to_vec())
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
    let pure_arg = |&a: &TNodeId| is_pure_value(l, a);
    matches!(l.tpd.tree(qualifier), Tree::New { .. }) && stable && l.tpd.items(args).iter().all(pure_arg)
}
