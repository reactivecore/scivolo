//! Returns of blocks as blocks ending in a return (dotty's `LambdaLift.transformReturn`).

use super::walk::walk_class_up;
use crate::Lower;
use syntax::ast::Span;
use typer::tpd::{TNodeId, Tree, Walk};

/// Makes each `return { stats; value }` in class definition `td` `{ stats; return value }`, the
/// block at the return's span, inner returns first.
pub fn transform(l: &mut Lower, td: TNodeId) {
    walk_class_up(l, td, &mut |_, _, _| Walk::Children, &mut |l, t, _| {
        if let Tree::Return { expr, from } = *l.tpd.tree(t)
            && let Tree::Block { stats, expr: value } = *l.tpd.tree(expr)
        {
            let tpe = l.tpd.get(t).tpe;
            let ret = l.tpd.node(Tree::Return { expr: value, from }, Span::NONE, tpe, None);
            let ret = l.positioned(ret);
            l.tpd.get_mut(t).tree = Tree::Block { stats, expr: ret };
        }
        Walk::Children
    });
}
