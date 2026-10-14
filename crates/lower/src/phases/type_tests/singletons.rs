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

/// The value of singleton type `tp` without a span: a reference, `this` or a literal (dotty's
/// `singleton`).
fn singleton(l: &mut Lower, tp: TypeId) -> Option<TNodeId> {
    match l.u.types().get(tp).clone() {
        Type::This(c) => {
            let c = l.u.class_symbol(c)?;
            Some(l.this(c, Span::NONE))
        }
        Type::Constant(..) | Type::Str(_) => {
            let lit = literal(l.u.types().get(tp))?;
            Some(l.tpd.node(lit, Span::NONE, Some(tp), None))
        }
        Type::Sym(prefix, s) if l.u.symbol(s).kind.is_term() => Some(reference(l, prefix, s)),
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
        Some((p, Type::Sym(_, q))) if l.u.symbol(q).kind.is_term() => singleton(l, p),
        Some((p, Type::TermRef(..))) => singleton(l, p),
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
