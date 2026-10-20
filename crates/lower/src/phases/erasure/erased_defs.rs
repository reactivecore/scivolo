// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/transform/Erasure.scala
//! The definitions erasure drops (dotty's `isEffectivelyErased` and `erasedDef`): inline methods
//! and values.

use super::super::walk::subtrees;
use crate::Lower;
use crate::symbols::has;
use tasty::tags::INLINE;
use typer::tpd::{TNodeId, Tree};

/// Drops the inline definitions of the class bodies and blocks in unit `root`; the definitions
/// dropped.
pub fn drop(l: &mut Lower, root: TNodeId) -> Vec<TNodeId> {
    let mut dropped = Vec::new();
    for t in subtrees(l.tpd, root) {
        let (Tree::Block { stats: list, .. } | Tree::Template { body: list, .. }) = *l.tpd.tree(t) else {
            continue;
        };
        let (erased, kept): (Vec<TNodeId>, Vec<TNodeId>) =
            l.tpd.items(list).iter().partition(|&&s| is_effectively_erased(l, s));
        if erased.is_empty() {
            continue;
        }
        dropped.extend(erased);
        let kept = l.tpd.list(&kept);
        match &mut l.tpd.get_mut(t).tree {
            Tree::Block { stats, .. } => *stats = kept,
            Tree::Template { body, .. } => *body = kept,
            _ => unreachable!("a block or template"),
        }
    }
    dropped
}

/// Whether `stat` defines an inline method or value.
fn is_effectively_erased(l: &Lower, stat: TNodeId) -> bool {
    matches!(l.tpd.tree(stat), Tree::DefDef { .. } | Tree::ValDef { .. })
        && l.tpd.sym(stat).is_some_and(|s| has(l.u, s, INLINE))
}
