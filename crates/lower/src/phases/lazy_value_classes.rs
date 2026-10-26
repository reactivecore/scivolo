// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/LazyVals.scala
//! Local lazy values of value classes, held as their underlying values as dotty's `LazyVals`
//! after erasure holds them.

use crate::definitions::type_ref;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::{SymbolId, TypeId};
use typer::tpd::{TNodeId, Tree};

/// The value class of `info`, its underlying value and that value's type, if `info` is one.
pub fn underlying(l: &mut Lower, info: TypeId, span: Span) -> Result<Option<(SymbolId, SymbolId, TypeId)>> {
    let Some(class) = l.u.value_class(info) else { return Ok(None) };
    if !l.u.type_params(class).is_empty() {
        return unsupported(span, "local lazy value of a generic value class");
    }
    let value = l.u.underlying(class).expect("a value class's value");
    let tpe = l.u.info(value);
    Ok(Some((class, value, tpe)))
}

/// `rhs.value` of type `tpe`, selected in the result of a block or the branches of an `if`.
pub fn unwrapped(l: &mut Lower, rhs: TNodeId, value: SymbolId, tpe: TypeId) -> TNodeId {
    match *l.tpd.tree(rhs) {
        Tree::Block { stats, expr } => {
            let expr = unwrapped(l, expr, value, tpe);
            let node = l.tpd.get_mut(rhs);
            (node.tree, node.tpe) = (Tree::Block { stats, expr }, Some(tpe));
            rhs
        }
        Tree::If { cond, thenp, elsep, inline } => {
            let (thenp, elsep) = (unwrapped(l, thenp, value, tpe), unwrapped(l, elsep, value, tpe));
            let node = l.tpd.get_mut(rhs);
            (node.tree, node.tpe) = (Tree::If { cond, thenp, elsep, inline }, Some(tpe));
            rhs
        }
        _ => {
            let span = l.tpd.span(rhs);
            let t = l.select(rhs, value, span);
            l.tpd.get_mut(t).tpe = Some(tpe);
            t
        }
    }
}

/// `new C(t)` for value class `class`.
pub fn wrapped(l: &mut Lower, class: SymbolId, t: TNodeId) -> TNodeId {
    let span = l.tpd.span(t);
    let new = l.new_(class, span);
    let ctor = l.u.decl(class, "<init>").unwrap_or_default();
    let ctor = *ctor.first().expect("a value class's constructor");
    let fun = l.select(new, ctor, span);
    let tpe = type_ref(l.u, class);
    l.apply(fun, &[t], tpe, span)
}
