//! Erasure's adaptation of a typed tree to its expected erased type: parameterless methods
//! applied, primitive values and `()` boxed where a reference is expected.

use crate::Lower;
use crate::symbols::{has, is_class, is_constructor, is_java_static, is_module};
use syntax::ast::Span;
use tasty::tags::FIELDaccessor;
use tasty::{Erased, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Whether `t` refers to a method without parameter lists: a getter, a value of a class other
/// than a field, or a parameterless `def`.
pub fn is_parameterless_ref(l: &mut Lower, t: TNodeId) -> bool {
    let (Tree::Select { .. } | Tree::Ident { .. }, Some(s)) = (l.tpd.tree(t), l.tpd.sym(t)) else {
        return false;
    };
    let in_class = l.u.symbol(s).owner.is_some_and(|o| is_class(l.u, o));
    let class_value = l.u.symbol(s).kind == Kind::Val
        && in_class
        && !is_module(l.u, s)
        && !l.fields.contains_key(&s)
        && s != l.defs.boxed_unit_unit;
    let method = l.u.symbol(s).kind == Kind::Def || has(l.u, s, FIELDaccessor) || class_value;
    let info = l.u.info(s);
    method
        && !is_constructor(l.u, s)
        && !matches!(l.u.types().get(info), Type::Method { .. } | Type::Poly { .. })
}

/// The erased type of `t` as erasure leaves it: a call's erased result type, a value's erased
/// declared type, an ascription's erasure, else the erasure of its type.
pub fn erased_type(l: &mut Lower, t: TNodeId) -> Erased {
    let sym = l.tpd.sym(t);
    match *l.tpd.tree(t) {
        Tree::Apply { fun, .. } => match l.tpd.sym(fun) {
            Some(m) if !is_constructor(l.u, m) => l.u.erased_method(m).1,
            _ => l.u.erase(l.tpd.tpe(t)),
        },
        Tree::Block { expr, .. } => erased_type(l, expr),
        Tree::Typed { tpt, .. } => l.u.erase(l.tpd.tpe(tpt)),
        Tree::Ident { .. } | Tree::Select { .. } if sym == Some(l.defs.boxed_unit_unit) => {
            Erased::Class("scala.runtime.BoxedUnit".into())
        }
        Tree::Ident { .. } | Tree::Select { .. } => match sym {
            Some(s) if !is_module(l.u, s) && l.u.symbol(s).kind.is_term() => {
                let info = l.u.info(s);
                l.u.erase(info)
            }
            _ => l.u.erase(l.tpd.tpe(t)),
        },
        _ => l.u.erase(l.tpd.tpe(t)),
    }
}

/// `span` with its start as point, as a tree made without a span takes from its parts.
fn from_start(span: Span) -> Span {
    if span.exists() { span.with_point(span.start()) } else { span }
}

/// `t()`, of `t`'s result type, spanning `t` with its start as point, unless `t` refers to a Java
/// static, which stands for a JDK field.
pub fn applied_to_none(l: &mut Lower, t: TNodeId) -> TNodeId {
    let span = match l.tpd.sym(t) {
        Some(s) if is_java_static(l.u, s) => l.tpd.span(t),
        _ => from_start(l.tpd.span(t)),
    };
    let widened = l.u.widen(l.tpd.tpe(t));
    let tpe = match l.u.types().get(widened).clone() {
        tasty::Type::Method { params, result, .. } if params.is_empty() => result,
        _ => widened,
    };
    l.apply(t, &[], tpe, span)
}

/// `t` adapted to erased type `pt`: boxed if a primitive value or `()` where a reference is
/// expected.
pub fn adapt(l: &mut Lower, t: TNodeId, pt: &Erased) -> TNodeId {
    if !matches!(pt, Erased::Class(_) | Erased::Array(_)) {
        return t;
    }
    match erased_type(l, t) {
        Erased::Prim(c) => match l.defs.primitive(c) {
            Some(p) => {
                let (module, box_method) = (p.module, p.box_method);
                boxed(l, t, module, box_method)
            }
            None => t,
        },
        Erased::Unit => boxed_unit(l, t),
        _ => t,
    }
}

/// `C.box(t)` for object `C` with method `box_method`.
fn boxed(l: &mut Lower, t: TNodeId, module: SymbolId, box_method: SymbolId) -> TNodeId {
    let span = from_start(l.tpd.span(t));
    let module_ref = l.ident(module, span);
    let fun = l.select(module_ref, box_method, span);
    let info = l.u.info(box_method);
    let result = l.u.result_type(info);
    l.apply(fun, &[t], result, span)
}

/// `BoxedUnit.UNIT` for unit value `t`, after `t` unless it is pure.
fn boxed_unit(l: &mut Lower, t: TNodeId) -> TNodeId {
    let span = from_start(l.tpd.span(t));
    let unit = l.defs.boxed_unit_unit;
    let unit = l.ident(unit, span);
    match l.tpd.tree(t) {
        Tree::Literal { .. } | Tree::Ident { .. } | Tree::This { .. } => unit,
        _ => l.block(&[t], unit, span),
    }
}
