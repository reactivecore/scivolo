//! Erasure's adaptation of a typed tree to its expected erased type: parameterless methods
//! applied, values boxed, unboxed or cast.

mod boxing;
mod units;
mod value;

use super::casts::{cast, conforms, is_cast};
use super::select_static::is_static_enum_value;
use crate::symbols::{has, is_java_static};
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::FIELDaccessor;
use tasty::{Erased, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};
use units::boxed_unit;

/// Whether `t` refers to a method without parameter lists: a getter, a value of a class other
/// than a field, or a parameterless `def`.
pub fn is_parameterless_ref(l: &mut Lower, t: TNodeId) -> bool {
    let (Tree::Select { .. } | Tree::Ident { .. }, Some(s)) = (l.tpd.tree(t), l.tpd.sym(t)) else {
        return false;
    };
    let in_class = l.u.symbol(s).owner.is_some_and(|o| l.u.is_class(o));
    let class_value = matches!(l.u.symbol(s).kind, Kind::Val | Kind::Param)
        && in_class
        && !l.u.is_module(s)
        && !l.fields.contains_key(&s)
        && !is_static_enum_value(l, s)
        && s != l.defs.boxed_unit_unit;
    let method = l.u.symbol(s).kind == Kind::Def || has(l.u, s, FIELDaccessor) || class_value;
    let mut info = l.u.info(s);
    while let Type::Poly { result, .. } = *l.u.types().get(info) {
        info = result;
    }
    method
        && !l.u.is_constructor(s)
        && !is_class_of(l, s)
        && !matches!(l.u.types().get(info), Type::Method { .. })
}

/// Whether `s` is `Predef.classOf`, a constant rather than a call.
pub fn is_class_of(l: &Lower, s: tasty::SymbolId) -> bool {
    let sym = l.u.symbol(s);
    sym.name == "classOf" && sym.owner.is_some_and(|o| l.u.full_name(o) == "scala.Predef$")
}

/// The erased type of `t` as erasure leaves it: a call's erased result type, a value's erased
/// declared type, an ascription's erasure, else the erasure of its type.
pub fn erased_type(l: &mut Lower, t: TNodeId) -> Erased {
    let sym = l.tpd.sym(t);
    match *l.tpd.tree(t) {
        Tree::Apply { fun, .. } => match (array_op(l, fun), synchronized_type(l, fun), l.tpd.sym(fun)) {
            (Some((_, result)), _, _) | (None, Some(result), _) => result,
            (None, None, Some(m)) if !l.u.is_constructor(m) => l.u.erased_method_value(m).1,
            (None, None, Some(_)) => l.u.erase_boxed(l.tpd.tpe(t)),
            _ => l.u.erase_value(l.tpd.tpe(t)),
        },
        Tree::TypeApply { .. } if is_cast(l, t) => l.u.erase_boxed(l.tpd.tpe(t)),
        Tree::New { .. } => l.u.erase_boxed(l.tpd.tpe(t)),
        Tree::Block { expr, .. } => erased_type(l, expr),
        Tree::Typed { tpt, .. } => l.u.erase_value(l.tpd.tpe(tpt)),
        Tree::Ident { .. } | Tree::Select { .. } if sym == Some(l.defs.boxed_unit_unit) => {
            Erased::Class("scala.runtime.BoxedUnit".into())
        }
        Tree::Ident { .. } | Tree::Select { .. } => match sym {
            Some(s) if !l.u.is_module(s) && l.u.symbol(s).kind.is_term() => {
                let info = l.u.info(s);
                match l.u.erase_value(info) {
                    Erased::Unit if matches!(l.u.symbol(s).kind, Kind::Val | Kind::Param) => {
                        Erased::Class("scala.runtime.BoxedUnit".into())
                    }
                    e => e,
                }
            }
            _ => l.u.erase_value(l.tpd.tpe(t)),
        },
        _ => l.u.erase_value(l.tpd.tpe(t)),
    }
}

/// The erased parameters and result of array operation `fun`, a selection of `apply`, `update`
/// or `length` on an array of a known element type; `None` for another function.
pub fn array_op(l: &mut Lower, fun: TNodeId) -> Option<(Vec<Erased>, Erased)> {
    let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { return None };
    let m = l.tpd.sym(fun)?;
    let owner = l.u.symbol(m).owner?;
    if l.u.full_name(owner) != "scala.Array" {
        return None;
    }
    let array = l.u.widen(l.tpd.tpe(qualifier));
    let Erased::Array(elem) = l.u.erase(array) else { return None };
    let int = Erased::Prim('i');
    match l.u.symbol(m).name.as_str() {
        "apply" => Some((vec![int], *elem)),
        "update" => Some((vec![int, *elem], Erased::Unit)),
        "length" => Some((Vec::new(), int)),
        _ => None,
    }
}

/// Whether `fun` applies `synchronized` to a type, which stays polymorphic after erasure.
pub fn is_synchronized(l: &Lower, fun: TNodeId) -> bool {
    matches!(*l.tpd.tree(fun),
        Tree::TypeApply { fun: sel, .. } if l.tpd.sym(sel) == Some(l.defs.synchronized))
}

/// The erased type argument of `synchronized` application `fun`, `BoxedUnit` for `Unit`; `None`
/// for another function.
pub fn synchronized_type(l: &mut Lower, fun: TNodeId) -> Option<Erased> {
    let Tree::TypeApply { args, .. } = *l.tpd.tree(fun) else { return None };
    let &[targ] = l.tpd.items(args) else { return None };
    is_synchronized(l, fun).then(|| match l.u.erase(l.tpd.tpe(targ)) {
        Erased::Unit => Erased::Class("scala.runtime.BoxedUnit".into()),
        e => e,
    })
}

/// Makes identifier `sel` of `synchronized` its selection on the `this` of its type's prefix, as
/// nscplugin's `DesugaredSelect` sees it.
pub fn qualify_synchronized(l: &mut Lower, sel: TNodeId) -> Result<()> {
    let Tree::Ident { name } = *l.tpd.tree(sel) else { return Ok(()) };
    let span = l.tpd.span(sel);
    let class = match *l.u.types().get(l.tpd.tpe(sel)) {
        Type::Sym(Some(p), _) | Type::TermRef(p, _) => match *l.u.types().get(p) {
            Type::This(c) => l.u.class_symbol(c),
            _ => None,
        },
        _ => None,
    };
    let Some(class) = class else { return unsupported(span, "`synchronized` on a prefix other than `this`") };
    let qualifier = l.this(class, span);
    l.tpd.get_mut(sel).tree = Tree::Select { qualifier, name };
    Ok(())
}

/// `span` with its start as point, as a tree made without a span takes from its parts.
pub fn from_start(span: Span) -> Span {
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

/// `t` adapted to erased type `pt` (dotty's `Boxing.adaptToType`): boxed if a primitive value or
/// `()` where a reference is expected, unboxed if a reference where a primitive is, cast if a
/// reference of a class that does not conform.
pub fn adapt(l: &mut Lower, t: TNodeId, pt: &Erased) -> TNodeId {
    let from = erased_type(l, t);
    value::adapt_from(l, t, from, pt)
}

/// `adapt` between erased types without value classes.
fn adapt_plain(l: &mut Lower, t: TNodeId, from: &Erased, pt: &Erased) -> TNodeId {
    let reference = |e: &Erased| matches!(e, Erased::Class(_) | Erased::Array(_));
    if matches!(from, Erased::Prim(_))
        && reference(pt)
        && let Some(arg) = boxing::removable_unbox_arg(l, t)
    {
        return arg;
    }
    match (from, pt) {
        (Erased::Prim(c), pt) if reference(pt) => match l.primitive(*c) {
            Some(p) => {
                let (module, box_method) = (p.module, p.box_method);
                converted(l, t, module, box_method)
            }
            None => t,
        },
        (Erased::Unit, pt) if reference(pt) => boxed_unit(l, t),
        (Erased::Class(_), Erased::Prim(c)) => match l.primitive(*c) {
            Some(p) => {
                let (module, unbox_method) = (p.module, p.unbox_method);
                converted(l, t, module, unbox_method)
            }
            None => t,
        },
        (from, Erased::Unit) if reference(from) || matches!(from, Erased::Prim(_)) => units::discarded(l, t),
        (from, to) if reference(from) && reference(to) && !conforms(l, from, to) => cast(l, t, to),
        _ => t,
    }
}

/// `t: tpe`, of type `tpe`, at `span`.
pub fn ascribed(l: &mut Lower, t: TNodeId, tpe: tasty::TypeId, span: Span) -> TNodeId {
    let tpt = l.type_tree(tpe, Span::NONE);
    l.tpd.node(Tree::Typed { expr: t, tpt }, span, Some(tpe), None)
}

/// `C.m(t)` for object `C` with method `m`.
pub fn converted(l: &mut Lower, t: TNodeId, module: SymbolId, m: SymbolId) -> TNodeId {
    let span = from_start(l.tpd.span(t));
    let module_ref = l.ident(module, span);
    let fun = l.select(module_ref, m, span);
    let info = l.u.info(m);
    let result = l.u.result_type(info);
    l.apply(fun, &[t], result, span)
}
