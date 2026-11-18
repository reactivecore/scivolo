// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/DropParentRefinements.scala
//! The members a class has for the refined values of its parents dropped from its template, as
//! `d` in `class Bar extends Foo("bar")` for `tracked val d` of `Foo` (dotty's
//! `DropParentRefinements`).

use crate::Lower;
use crate::symbols::has;
use tasty::tags::{SYNTHETIC, TRACKED};
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, classes: &[TNodeId]) {
    for &td in classes {
        let Tree::TypeDef { rhs, .. } = *l.tpd.tree(td) else { continue };
        let Tree::Template { constr, parents, self_, body } = l.tpd.tree(rhs).clone() else { continue };
        let items = l.tpd.items(body).to_vec();
        let kept: Vec<TNodeId> = items.iter().copied().filter(|&m| !is_refinement(l, m)).collect();
        if kept.len() != items.len() {
            let body = l.tpd.list(&kept);
            l.tpd.get_mut(rhs).tree = Tree::Template { constr, parents, self_, body };
        }
    }
}

/// Whether member definition `m` is that of a refined value of a parent.
fn is_refinement(l: &Lower, m: TNodeId) -> bool {
    matches!(l.tpd.tree(m), Tree::ValDef { .. } | Tree::DefDef { .. })
        && l.tpd.sym(m).is_some_and(|s| l.u.is_deferred(s) && has(l.u, s, SYNTHETIC) && has(l.u, s, TRACKED))
}
