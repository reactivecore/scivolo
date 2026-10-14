//! `FirstTransform`'s `foldCondition`: `true && y` is `y`, `false && y` is `false`, `true || y`
//! is `true`, `false || y` is `y`.

use crate::Lower;
use syntax::ast::constant::Constant;
use typer::tpd::{TNodeId, Tree};

/// Replaces `t` by the operand it reduces to, if it is `&&` or `||` on a boolean literal.
pub fn fold(l: &mut Lower, t: TNodeId) {
    let Tree::Apply { fun, args } = *l.tpd.tree(t) else { return };
    let &[y] = l.tpd.items(args) else { return };
    let Tree::Select { qualifier: x, .. } = *l.tpd.tree(fun) else { return };
    let Tree::Literal { value: Constant::Boolean(c) } = l.tpd.tree(x) else { return };
    let c = *c;
    let and = match l.tpd.sym(fun) {
        Some(op) if op == l.defs.patmat.and => true,
        Some(op) if op == l.defs.case_classes.or => false,
        _ => return,
    };
    l.copy_node(t, if c == and { y } else { x });
}
