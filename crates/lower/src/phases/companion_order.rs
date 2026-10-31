// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/FirstTransform.scala
//! The statement order of dotty's `FirstTransform.reorderAndComplete`: the class of an object
//! moves after its companion class when that class follows it.

use super::walk::{is_class_def, subtrees};
use crate::Lower;
use tasty::module_name;
use syntax::source::Offset;
use typer::tpd::{List, TNodeId, Tree};

/// Reorders the statements of every package, template and block of unit `root`.
pub fn transform(l: &mut Lower, root: TNodeId) {
    for t in subtrees(l.tpd, root) {
        match *l.tpd.tree(t) {
            Tree::PackageDef { pid, stats } => {
                if let Some(stats) = reorder(l, stats) {
                    l.tpd.get_mut(t).tree = Tree::PackageDef { pid, stats };
                }
            }
            Tree::Template { constr, parents, self_, body } => {
                if let Some(body) = reorder(l, body) {
                    l.tpd.get_mut(t).tree = Tree::Template { constr, parents, self_, body };
                }
            }
            Tree::Block { stats, expr } => {
                if let Some(stats) = reorder(l, stats) {
                    l.tpd.get_mut(t).tree = Tree::Block { stats, expr };
                }
            }
            _ => {}
        }
    }
}

/// `stats` with each module class moved after the first companion class that follows it, if
/// one moved.
fn reorder(l: &mut Lower, stats: List) -> Option<List> {
    let stats = l.tpd.items(stats).to_vec();
    let class = |l: &Lower, t: TNodeId| l.tpd.sym(t).filter(|_| is_class_def(l.tpd, t));
    let mut after: Vec<Option<TNodeId>> = vec![None; stats.len()];
    let mut moved = vec![false; stats.len()];
    for (i, &m) in stats.iter().enumerate() {
        let Some(mc) = class(l, m).filter(|&c| l.u.is_module_class(c)) else { continue };
        let Some(name) = module_name(&l.u.symbol(mc).name).map(str::to_owned) else { continue };
        let companion = stats.iter().enumerate().skip(i + 1).find(|&(j, &c)| {
            after[j].is_none()
                && class(l, c).is_some_and(|c| !l.u.is_module_class(c) && l.u.symbol(c).name == name)
        });
        if let Some((j, &c)) = companion {
            after[j] = Some(m);
            moved[i] = true;
            let (m, c) = (l.tpd.span(m), l.tpd.span(c));
            if m.exists() && c.exists() && m.end() <= c.end() {
                l.moved_classes.push((m, c.end()));
            }
        }
    }
    if !moved.contains(&true) {
        return None;
    }
    let mut out = Vec::with_capacity(stats.len());
    for (i, &s) in stats.iter().enumerate() {
        if !moved[i] {
            out.push(s);
        }
        out.extend(after[i]);
    }
    Some(l.tpd.list(&out))
}

/// Offset `at` of the unit's source moved with the classes `reorder` moved, so that offsets
/// compare in the order of the reordered trees.
pub fn tree_offset(l: &Lower, mut at: Offset) -> Offset {
    for &(m, end) in l.moved_classes.iter().rev() {
        if (m.start()..m.end()).contains(&at) {
            at += end - m.end();
        } else if (m.end()..end).contains(&at) {
            at -= m.end() - m.start();
        }
    }
    at
}
