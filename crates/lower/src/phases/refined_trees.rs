//! Refined type trees as plain type trees of their types, their members' definitions dropped.

use super::walk::subtrees;
use crate::Lower;
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, root: TNodeId) {
    for t in subtrees(l.tpd, root) {
        if matches!(l.tpd.tree(t), Tree::RefinedTypeTree { .. }) {
            l.tpd.get_mut(t).tree = Tree::TypeTree { inferred: false };
        }
    }
}
