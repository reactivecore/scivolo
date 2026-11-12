// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/LambdaLift.scala
//! Moving the definitions of lifted local classes from their blocks to their packages, through
//! the parent of each tree of the unit, kept across the moves.

use super::walk::children;
use crate::Lower;
use rustc_hash::{FxHashMap, FxHashSet};
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// The trees of a unit by their parents, and its package definitions in `subtrees` order.
pub struct Placement {
    root: TNodeId,
    parent: FxHashMap<TNodeId, TNodeId>,
    package_defs: Vec<TNodeId>,
    /// Whether a tree has several parents, where moves walk the unit instead.
    shared: bool,
}

impl Placement {
    /// Finds the parent of each tree of unit `root`.
    pub fn of(l: &Lower, root: TNodeId) -> Self {
        let (trees, parents) = subtrees_with_parents(l, root);
        let mut placement =
            Placement { root, parent: FxHashMap::default(), package_defs: Vec::new(), shared: false };
        for (&t, p) in trees.iter().zip(parents) {
            if matches!(l.tpd.tree(t), Tree::PackageDef { .. }) {
                placement.package_defs.push(t);
            }
            if let Some(p) = p {
                placement.shared |= placement.parent.insert(t, trees[p]).is_some();
            }
        }
        placement
    }

    /// The trees from `td` up to the root, if each is still a child of the next.
    fn path(&self, l: &Lower, td: TNodeId) -> Option<Vec<TNodeId>> {
        let mut path = vec![td];
        while let Some(&last) = path.last()
            && last != self.root
        {
            let p = *self.parent.get(&last)?;
            if path.len() > self.parent.len() || !children(l.tpd, p).contains(&last) {
                return None;
            }
            path.push(p);
        }
        Some(path)
    }
}

/// Moves class definition `td` of `class` from its block to its package; the block and the index
/// of the statement it was before.
pub fn lift(
    l: &mut Lower,
    placement: &mut Placement,
    td: TNodeId,
    class: SymbolId,
) -> Option<(TNodeId, usize)> {
    let mut package = class;
    while l.u.symbol(package).kind != Kind::Package {
        package = l.u.symbol(package).owner.expect("a class's owner");
    }
    let owner = l.u.symbol(class).owner.expect("a class's owner");
    l.u.set_owner(class, package);
    l.u.enter(class);
    l.local_classes.insert(class, owner);
    let in_package = |l: &Lower, t: TNodeId| {
        matches!(*l.tpd.tree(t), Tree::PackageDef { pid, .. }
            if l.tpd.sym(t) == Some(package) || l.tpd.sym(pid) == Some(package))
    };
    let path = (!placement.shared).then(|| placement.path(l, td)).flatten();
    let target = path.as_ref().and_then(|path| {
        let mut defs = placement.package_defs.iter().copied().filter(|&t| in_package(l, t));
        defs.rfind(|t| path.contains(t))
    });
    let (Some(path), Some(target)) = (path, target) else {
        let from = lift_walking(l, placement.root, td, in_package);
        if !placement.shared {
            *placement = Placement::of(l, placement.root);
        }
        return from;
    };
    let from = path.get(1).and_then(|&b| remove_stat(l, b, td).map(|index| (b, index)));
    append_stat(l, target, td);
    match from {
        Some(_) => _ = placement.parent.insert(td, target),
        None => placement.shared = true,
    }
    from
}

/// `lift` by a walk of unit `root`, for trees of several parents.
fn lift_walking(
    l: &mut Lower,
    root: TNodeId,
    td: TNodeId,
    in_package: impl Fn(&Lower, TNodeId) -> bool,
) -> Option<(TNodeId, usize)> {
    let mut from = None;
    let (trees, parents) = subtrees_with_parents(l, root);
    let package_defs: Vec<TNodeId> = trees.iter().copied().filter(|&t| in_package(l, t)).collect();
    let mut around = FxHashSet::default();
    for i in (0..trees.len()).filter(|&i| trees[i] == td) {
        let mut at = Some(i);
        while let Some(i) = at {
            around.insert(trees[i]);
            at = parents[i];
        }
    }
    let enclosing = package_defs.iter().rfind(|&t| around.contains(t));
    let target = enclosing.or(package_defs.first()).copied();
    for t in trees {
        match *l.tpd.tree(t) {
            Tree::Block { .. } => {
                if let Some(index) = remove_stat(l, t, td) {
                    from = Some((t, index));
                }
            }
            Tree::PackageDef { .. } if Some(t) == target => append_stat(l, t, td),
            _ => {}
        }
    }
    from
}

/// Removes statement `td` from block `b`; its index.
fn remove_stat(l: &mut Lower, b: TNodeId, td: TNodeId) -> Option<usize> {
    let Tree::Block { stats, expr } = *l.tpd.tree(b) else { return None };
    let mut kept = l.tpd.items(stats).to_vec();
    let index = kept.iter().position(|&s| s == td)?;
    kept.remove(index);
    let stats = l.tpd.list(&kept);
    l.tpd.get_mut(b).tree = Tree::Block { stats, expr };
    Some(index)
}

/// Appends statement `td` to package definition `p`.
fn append_stat(l: &mut Lower, p: TNodeId, td: TNodeId) {
    let Tree::PackageDef { pid, stats } = *l.tpd.tree(p) else { return };
    let mut all = l.tpd.items(stats).to_vec();
    all.push(td);
    let stats = l.tpd.list(&all);
    l.tpd.get_mut(p).tree = Tree::PackageDef { pid, stats };
}

/// Finds `root` and the trees in it in `subtrees` order, with the index of each one's parent.
fn subtrees_with_parents(l: &Lower, root: TNodeId) -> (Vec<TNodeId>, Vec<Option<usize>>) {
    let (mut trees, mut parents) = (vec![root], vec![None]);
    let mut i = 0;
    while i < trees.len() {
        for c in children(l.tpd, trees[i]) {
            trees.push(c);
            parents.push(Some(i));
        }
        i += 1;
    }
    (trees, parents)
}
