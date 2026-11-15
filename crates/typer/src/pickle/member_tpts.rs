//! The type trees of a super accessor's member, whose annotation trees the accessor's parameter
//! and result types take.

use crate::tpd::{TNodeId, Tpd, Tree as T};

/// Collects the typed trees of each term parameter's definition of method definition `def`, in
/// order, and of its result type tree; `None` if `def` is no method definition.
pub fn member_tpts(tpd: &Tpd, def: TNodeId) -> Option<Vec<Vec<TNodeId>>> {
    let T::DefDef { tpt, .. } = *tpd.tree(def) else { return None };
    let params = tpd.children(def).into_iter().filter(|&c| matches!(tpd.tree(c), T::ValDef { .. }));
    Some(params.chain([tpt]).map(|t| subtree(tpd, t)).collect())
}

/// The typed trees of `t` and its descendants.
fn subtree(tpd: &Tpd, t: TNodeId) -> Vec<TNodeId> {
    let (mut todo, mut trees) = (vec![t], Vec::new());
    while let Some(t) = todo.pop() {
        if tpd.get(t).tpe.is_some() {
            trees.push(t);
        }
        todo.extend(tpd.children(t));
    }
    trees
}
