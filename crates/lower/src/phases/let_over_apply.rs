//! dotty's `LetOverApply`: `{ stats; expr }.f(args)` becomes `{ stats; expr.f(args) }` unless
//! `expr` is a closure, and `{ stats; expr }(args)` becomes `{ stats; expr(args) }`.

use super::drop_inlined::is_inlined;
use super::walk::walk_class_up;
use crate::Lower;
use std::ops::ControlFlow;
use typer::tpd::{TNodeId, Tree, Walk};

/// Lifts the blocks out of the receivers of the calls in class definition `td`, inner calls first.
pub fn transform(l: &mut Lower, td: TNodeId) {
    walk_class_up(l, td, &mut |_, _, _| Walk::Children, &mut |l, t, _| {
        lift(l, t);
        ControlFlow::Continue(())
    });
}

/// Makes call `t` the block of its receiver, ending in the call.
fn lift(l: &mut Lower, t: TNodeId) {
    let Tree::Apply { fun, args } = *l.tpd.tree(t) else { return };
    let (block, fun) = match *l.tpd.tree(fun) {
        _ if is_inlined(l, fun) || l.applied_references.contains(&t) => return,
        Tree::Select { qualifier, name } => {
            let Tree::Block { expr, .. } = *l.tpd.tree(qualifier) else { return };
            if matches!(l.tpd.tree(expr), Tree::Closure { .. }) || is_inlined(l, qualifier) {
                return;
            }
            l.tpd.get_mut(fun).tree = Tree::Select { qualifier: expr, name };
            (qualifier, fun)
        }
        Tree::Block { expr, .. } => (fun, expr),
        _ => return,
    };
    let Tree::Block { stats, .. } = *l.tpd.tree(block) else { unreachable!("a block") };
    let node = l.tpd.get(t);
    let (span, tpe, sym) = (node.span, node.tpe, node.sym);
    let call = l.tpd.node(Tree::Apply { fun, args }, span, tpe, sym);
    let block_span = l.tpd.span(block);
    let node = l.tpd.get_mut(t);
    (node.tree, node.span, node.sym) = (Tree::Block { stats, expr: call }, block_span, None);
}
