// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//   compiler/src/dotty/tools/dotc/transform/Erasure.scala
//! `Unit` values in erased trees: boxed as `BoxedUnit.UNIT`, or a value discarded for `()`
//! (dotty's `Boxing.box` and `unbox` for `Unit`); ascriptions of types erasing to `Unit`.

use super::from_start;
use crate::Lower;
use crate::symbols::has;
use tasty::tags::{LAZY, MUTABLE};
use tasty::{Erased, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// The erasure of ascribed type `tpt`: `BoxedUnit` for a type erasing to `Unit` whose type symbol
/// is not `Unit`, a value class boxed if `tpt` is a boxed ascription's (dotty's `typedTyped`).
pub fn ascription_type(l: &mut Lower, tpt: TNodeId) -> Erased {
    let tpe = l.tpd.tpe(tpt);
    if l.boxed_types.contains(&tpt) {
        return l.u.erase_boxed(tpe);
    }
    match l.u.erase_value(tpe) {
        Erased::Unit if type_symbol(l, tpe) != Some(l.defs.unit) => {
            Erased::Class("scala.runtime.BoxedUnit".into())
        }
        e => e,
    }
}

/// The type symbol `t` refers to, through proxies but not aliases (dotty's `typeSymbol`).
fn type_symbol(l: &mut Lower, mut t: TypeId) -> Option<SymbolId> {
    for _ in 0..64 {
        t = match *l.u.types().get(t) {
            Type::Sym(_, s) if l.u.is_type_symbol(s) => return Some(s),
            Type::TypeRef(..) => return l.u.resolve_ref(t),
            Type::Annotated(inner, _)
            | Type::Refined(inner, ..)
            | Type::Applied(inner, _)
            | Type::Flexible(inner) => inner,
            _ => l.u.widen_once(t)?,
        };
    }
    None
}

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
/// literal, `this`, an object, a parameter or immutable value of a method, or an immutable field
/// without a getter.
fn is_pure(l: &Lower, t: TNodeId) -> bool {
    let local = |s| {
        let sym = l.u.symbol(s);
        let in_method = sym.owner.is_some_and(|o| !l.u.is_class(o));
        let value =
            sym.kind == Kind::Param || sym.kind == Kind::Val && !has(l.u, s, LAZY) && !has(l.u, s, MUTABLE);
        value && in_method
    };
    let field = |s| l.fields.get(&s) == Some(&s) && !has(l.u, s, MUTABLE) && !has(l.u, s, LAZY);
    match *l.tpd.tree(t) {
        Tree::Literal { .. } | Tree::ClassLiteral { .. } | Tree::This { .. } => true,
        Tree::Ident { .. } => l.tpd.sym(t).is_some_and(|s| l.u.is_module(s) || local(s) || field(s)),
        Tree::Select { qualifier, .. } => {
            l.tpd.sym(t).is_some_and(|s| l.u.is_module(s) || field(s)) && is_pure(l, qualifier)
        }
        _ => false,
    }
}
