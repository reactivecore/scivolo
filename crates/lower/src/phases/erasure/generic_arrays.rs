//! Members of arrays of an element type erasure does not know, as `ScalaRunTime`'s
//! `array_apply`, `array_update`, `array_length` and `array_clone` (dotty's `selectArrayMember`).

use crate::Lower;
use crate::phases::adapt::from_start;
use syntax::ast::Span;
use tasty::Erased;
use typer::tpd::{TNodeId, Tree};

/// For selection `sel` of an `Array` member on a receiver erasing to `Object`: the reference to
/// `ScalaRunTime`'s method for it, the object at the receiver's start; and the receiver, the
/// method's first argument.
pub fn runtime_op(l: &mut Lower, sel: TNodeId) -> Option<(TNodeId, TNodeId)> {
    let Tree::Select { qualifier, .. } = *l.tpd.tree(sel) else { return None };
    let m = l.tpd.sym(sel)?;
    let owner = l.u.symbol(m).owner?;
    if l.u.full_name(owner) != "scala.Array" || l.u.is_constructor(m) {
        return None;
    }
    let array = l.u.widen(l.tpd.tpe(qualifier));
    if matches!(l.u.erase(array), Erased::Array(_)) {
        return None;
    }
    let module = l.scala_run_time().ok()?;
    let class = l.u.module_class(module)?;
    let name = format!("array_{}", l.u.symbol(m).name);
    let method = *l.u.decl(class, &name).ok()?.first()?;
    let module = l.ident(module, from_start(l.tpd.span(qualifier)));
    Some((l.select(module, method, Span::NONE), qualifier))
}

/// Selection `t` of `length` or `clone` on a generic array made the call of `ScalaRunTime`'s
/// method for it, in place, spanning `t` with its start as point.
pub fn rewrite_select(l: &mut Lower, t: TNodeId) -> bool {
    let Some((fun, qualifier)) = runtime_op(l, t) else { return false };
    let (tpe, span) = (l.tpd.tpe(t), from_start(l.tpd.span(t)));
    let call = l.apply(fun, &[qualifier], tpe, span);
    l.copy_node(t, call);
    true
}
