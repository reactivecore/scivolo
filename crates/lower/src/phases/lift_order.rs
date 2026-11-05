// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/LambdaLift.scala
//! The order of the methods `LambdaLift` appends to a class: that of the traversal of the unit
//! that lifts them, a nested class's at its place among the statements of its class.

use crate::Lower;
use tasty::Kind;
use typer::tpd::{TNodeId, Tree};

/// The statements of class definition `td` whose functions are lifted, in order: the arguments
/// of its trait parents, then its body.
pub fn class_stats(l: &mut Lower, td: TNodeId) -> Vec<TNodeId> {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { return Vec::new() };
    let Tree::Template { parents, body, .. } = *l.tpd.tree(template) else { return Vec::new() };
    let mut stats = super::trait_params::trait_args(l, parents);
    stats.extend_from_slice(l.tpd.items(body));
    stats
}

/// The indices of the statements holding class definition `td`, from the outermost class among
/// `classes` down; `usize::MAX` for a class that is no statement of a class.
pub fn class_path(l: &mut Lower, mut td: TNodeId, classes: &[TNodeId]) -> Vec<usize> {
    let mut path = Vec::new();
    while let Some(owner) = l.tpd.sym(td).and_then(|c| l.u.symbol(c).owner)
        && l.u.symbol(owner).kind != Kind::Package
    {
        let Some(&outer) = classes.iter().find(|&&t| l.tpd.sym(t) == Some(owner)) else {
            path.push(usize::MAX);
            break;
        };
        let index = class_stats(l, outer).iter().position(|&s| s == td);
        path.push(index.unwrap_or(usize::MAX));
        td = outer;
    }
    path.reverse();
    path
}
