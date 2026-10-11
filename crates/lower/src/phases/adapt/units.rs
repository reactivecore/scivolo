//! `Unit` values in erased trees: boxed as `BoxedUnit.UNIT`, or a value discarded for `()`
//! (dotty's `Boxing.box` and `unbox` for `Unit`).

use super::from_start;
use crate::Lower;
use crate::symbols::has;
use tasty::Kind;
use tasty::tags::{LAZY, MUTABLE};
use typer::tpd::{TNodeId, Tree};

/// `BoxedUnit.UNIT` for unit value `t`, after `t` unless it is pure.
pub fn boxed_unit(l: &mut Lower, t: TNodeId) -> TNodeId {
    let span = from_start(l.tpd.span(t));
    let unit = l.defs.boxed_unit_unit;
    let unit = l.ident(unit, span);
    match l.tpd.tree(t) {
        Tree::Literal { .. } | Tree::Ident { .. } | Tree::This { .. } => unit,
        _ => l.block(&[t], unit, l.tpd.span(t)),
    }
}

/// `()` after `t` unless `t` is pure (dotty's `constant(t, unitLiteral)`).
pub fn discarded(l: &mut Lower, t: TNodeId) -> TNodeId {
    let span = l.tpd.span(t);
    let unit = l.unit_literal(span);
    if is_pure(l, t) { unit } else { l.block(&[t], unit, span) }
}

/// Whether evaluating erased tree `t` has no effect (dotty's `isPureExpr` after `Getters`): a
/// literal, `this`, an object, or a parameter or immutable value of a method.
fn is_pure(l: &Lower, t: TNodeId) -> bool {
    let local = |s| {
        let sym = l.u.symbol(s);
        let in_method = sym.owner.is_some_and(|o| !l.u.is_class(o));
        let value =
            sym.kind == Kind::Param || sym.kind == Kind::Val && !has(l.u, s, LAZY) && !has(l.u, s, MUTABLE);
        value && in_method
    };
    match *l.tpd.tree(t) {
        Tree::Literal { .. } | Tree::ClassLiteral { .. } | Tree::This { .. } => true,
        Tree::Ident { .. } => l.tpd.sym(t).is_some_and(|s| l.u.is_module(s) || local(s)),
        Tree::Select { qualifier, .. } => {
            l.tpd.sym(t).is_some_and(|s| l.u.is_module(s)) && is_pure(l, qualifier)
        }
        _ => false,
    }
}
