// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/FirstTransform.scala
//! The imports of a unit dropped from its templates, blocks and packages (dotty's
//! `FirstTransform` eliminating imports).

use super::walk::subtrees;
use crate::Lower;
use typer::tpd::{List, TNodeId, Tree};

pub fn transform(l: &mut Lower, root: TNodeId) {
    for t in subtrees(l.tpd, root) {
        let tree = match l.tpd.tree(t).clone() {
            Tree::Template { constr, parents, self_, body } => {
                Tree::Template { constr, parents, self_, body: without_imports(l, body) }
            }
            Tree::Block { stats, expr } => Tree::Block { stats: without_imports(l, stats), expr },
            Tree::PackageDef { pid, stats } => Tree::PackageDef { pid, stats: without_imports(l, stats) },
            _ => continue,
        };
        l.tpd.get_mut(t).tree = tree;
    }
}

fn without_imports(l: &mut Lower, stats: List) -> List {
    let items = l.tpd.items(stats);
    if !items.iter().any(|&s| matches!(l.tpd.tree(s), Tree::Import { .. })) {
        return stats;
    }
    let kept: Vec<TNodeId> =
        items.iter().copied().filter(|&s| !matches!(l.tpd.tree(s), Tree::Import { .. })).collect();
    l.tpd.list(&kept)
}
