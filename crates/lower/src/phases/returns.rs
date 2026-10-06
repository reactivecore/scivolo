//! Returns of blocks as blocks ending in a return (dotty's `LambdaLift.transformReturn`).

use super::walk::children;
use crate::Lower;
use syntax::ast::Span;
use typer::tpd::{TNodeId, Tree};

/// Makes each `return { stats; value }` in class definition `td` `{ stats; return value }`, the
/// block at the return's span, inner returns first.
pub fn transform(l: &mut Lower, td: TNodeId) {
    let mut returns = Vec::new();
    let mut todo = vec![td];
    while let Some(t) = todo.pop() {
        if matches!(l.tpd.tree(t), Tree::Return { .. }) {
            returns.push(t);
        }
        if t == td || !matches!(l.tpd.tree(t), Tree::TypeDef { .. }) {
            todo.extend(children(l.tpd, t));
        }
    }
    for &t in returns.iter().rev() {
        let Tree::Return { expr, from } = *l.tpd.tree(t) else { unreachable!("a return") };
        let Tree::Block { stats, expr: value } = *l.tpd.tree(expr) else { continue };
        let tpe = l.tpd.get(t).tpe;
        let ret = l.tpd.node(Tree::Return { expr: value, from }, Span::NONE, tpe, None);
        let ret = l.positioned(ret);
        l.tpd.get_mut(t).tree = Tree::Block { stats, expr: ret };
    }
}
