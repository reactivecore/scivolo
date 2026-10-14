//! Tests for singleton types: `expr.asInstanceOf[Object] eq s` for a reference `s`, `s == expr`
//! for a string constant or a value of another type (dotty's `tpd.isInstance`).

use crate::Lower;
use crate::phases::first_transform::literal;
use syntax::ast::Span;
use tasty::{Kind, Type, TypeId};
use typer::tpd::TNodeId;

/// `expr` tested for singleton type `tp`, already dealiased, whose value is `value` if given;
/// `None` if `tp` is no singleton.
pub fn is_instance(l: &mut Lower, expr: TNodeId, tp: TypeId, value: Option<TNodeId>) -> Option<TNodeId> {
    let value = match value {
        Some(v) => v,
        None => singleton(l, tp)?,
    };
    let object = l.u.class_ref("java.lang.Object");
    let widened = l.u.widen(tp);
    if matches!(l.u.types().get(tp), Type::Str(_)) || !l.u.is_subtype(widened, object) {
        return Some(l.equal(value, expr));
    }
    let expr = l.ensure_conforms(expr, object);
    Some(l.call(expr, l.defs.patmat.eq, &[value]))
}

/// Whether `tp` is the type of one value: a path or a constant.
pub fn is_singleton(l: &Lower, tp: TypeId) -> bool {
    match *l.u.types().get(tp) {
        Type::TermRef(..) | Type::This(_) | Type::Constant(..) | Type::Str(_) => true,
        Type::Sym(_, s) => l.u.symbol(s).kind.is_term(),
        _ => false,
    }
}

/// Whether dealiased type `tp` is or has as a part of its union or intersection `this.type` of a
/// value class, whose value its extension methods take as a parameter.
pub fn has_value_class_this(l: &mut Lower, tp: TypeId) -> bool {
    match l.u.types().get(tp).clone() {
        Type::Or(a, b, _) | Type::And(a, b) => {
            let (a, b) = (l.u.dealias(a), l.u.dealias(b));
            has_value_class_this(l, a) || has_value_class_this(l, b)
        }
        Type::This(c) => l.u.value_class(c).is_some(),
        _ => false,
    }
}

/// Whether `tp` is `EmptyTuple.type`.
pub fn is_empty_tuple(l: &mut Lower, tp: TypeId) -> bool {
    let s = match l.u.types().get(tp).clone() {
        Type::Sym(_, s) => Some(s),
        Type::TermRef(p, name) => l.u.resolve(p, &name, false),
        _ => None,
    };
    s.is_some_and(|s| l.u.full_name(s) == "scala.Tuple$package$.EmptyTuple")
}

/// The value of singleton type `tp` without a span: a reference, `this` or a literal (dotty's
/// `singleton`).
fn singleton(l: &mut Lower, tp: TypeId) -> Option<TNodeId> {
    if !is_singleton(l, tp) {
        return None;
    }
    match l.u.types().get(tp).clone() {
        Type::This(c) => {
            let c = l.u.class_symbol(c)?;
            Some(l.this(c, Span::NONE))
        }
        Type::Constant(..) | Type::Str(_) => {
            let lit = literal(l.u.types().get(tp))?;
            Some(l.tpd.node(lit, Span::NONE, Some(tp), None))
        }
        Type::Sym(prefix, s) => Some(reference(l, prefix, s)),
        Type::TermRef(prefix, name) => {
            let s = l.u.resolve(prefix, &name, false)?;
            Some(reference(l, Some(prefix), s))
        }
        _ => None,
    }
}

/// A reference to term `s` through `prefix` if that is a value (dotty's `ref`).
fn reference(l: &mut Lower, prefix: Option<TypeId>, s: tasty::SymbolId) -> TNodeId {
    let qualifier = match prefix.map(|p| (p, l.u.types().get(p).clone())) {
        Some((p, Type::Sym(..) | Type::TermRef(..))) => singleton(l, p),
        Some((p, Type::This(c))) => {
            let package = l.u.class_symbol(c).is_some_and(|c| l.u.symbol(c).kind == Kind::Package);
            (!package && !l.u.is_module(s)).then(|| singleton(l, p)).flatten()
        }
        _ => None,
    };
    match qualifier {
        Some(q) => l.select(q, s, Span::NONE),
        None => l.ident(s, Span::NONE),
    }
}
