//! `x.asInstanceOf[T]` as dotty's `transformAsInstanceOf` erases it.

use crate::phases::adapt::{adapt, ascribed, converted, erased_type};
use crate::phases::casts::{class_named, type_of};
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::{Erased, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// Cast `t` of `expr` to `target`: an ascription if `expr`'s type erases to `target`'s primitive,
/// `expr` then `()` for `Unit`, a conversion or a cast of the boxed value from a primitive,
/// unboxing from a reference or `null` to a primitive, a `ClassCastException` for `Nothing`,
/// else `t`; casts from or to value classes as `value_cast`.
pub fn cast(l: &mut Lower, t: TNodeId, expr: TNodeId, tpt: TNodeId) -> Result<TNodeId> {
    let target = l.tpd.tpe(tpt);
    let to = l.u.erase(target);
    let widened = l.u.widen(l.tpd.tpe(expr));
    let found = l.u.erase(widened);
    let from = erased_type(l, expr);
    let span = l.tpd.span(t);
    if l.u.value_class(target).is_some() || l.u.value_class(widened).is_some() {
        return value_cast(l, t, expr, tpt);
    }
    if l.u.erase_boxed(target) != to || l.u.erase_boxed(widened) != found {
        return unsupported(span, "casts to or from value classes");
    }
    let refused = || unsupported(span, "casts between primitive values or to references");
    let ascription = matches!((&found, &to), (Erased::Prim(a), Erased::Prim(b)) if a == b)
        || (matches!(found, Erased::Null | Erased::Unit) && to == Erased::Unit);
    Ok(match (&found, &to) {
        _ if ascription => l.tpd.node(Tree::Typed { expr, tpt }, Span::NONE, Some(target), None),
        (Erased::Null, Erased::Prim(c)) => match l.defs.primitive(*c) {
            Some(p) => {
                let (module, unbox_method) = (p.module, p.unbox_method);
                converted(l, expr, module, unbox_method)
            }
            None => return refused(),
        },
        (found, Erased::Unit) if *found != Erased::Unit => {
            let span = l.tpd.span(expr);
            let unit = l.unit_literal(Span::NONE);
            let block = l.block(&[expr], unit, Span::NONE);
            let block = l.positioned(block);
            l.with_span(block, span)
        }
        (Erased::Prim(a), Erased::Prim(b)) => match primitive_conversion(l, expr, widened, *a, *b) {
            Some(converted) => converted,
            None => return refused(),
        },
        (Erased::Prim(a), Erased::Class(_) | Erased::Array(_)) => match boxed_cast(l, t, expr, *a) {
            Some(t) => t,
            None => return refused(),
        },
        (Erased::Prim(_) | Erased::Unit, _) | (_, Erased::Unit) => return refused(),
        (_, Erased::Prim(_)) if matches!(from, Erased::Class(_) | Erased::Array(_)) => adapt(l, expr, &to),
        (_, Erased::Prim(_)) => return refused(),
        (_, Erased::Nothing) if matches!(from, Erased::Class(_) | Erased::Array(_) | Erased::Null) => {
            throw_class_cast(l, expr)
        }
        _ => t,
    })
}

/// Cast `t` of `expr` to `tpt` where either is a value class: an ascription if `expr`'s class
/// derives from `tpt`'s, the boxed primitive cast, the boxed value unboxed to a primitive,
/// `expr` for `adapt` to unbox if `tpt` is a value class, else `t` on the boxed value.
fn value_cast(l: &mut Lower, t: TNodeId, expr: TNodeId, tpt: TNodeId) -> Result<TNodeId> {
    let target = l.tpd.tpe(tpt);
    let widened = l.u.widen(l.tpd.tpe(expr));
    let (test, found) = (l.u.erase_boxed(target), l.u.erase_boxed(widened));
    let span = l.tpd.span(t);
    let derives = match (&found, &test) {
        (Erased::Class(f), Erased::Class(t)) => match (class_named(l, f), class_named(l, t)) {
            (Some(f), Some(t)) => l.u.derives_from(f, t),
            _ => false,
        },
        _ => false,
    };
    if derives {
        let tpe = type_of(l, &test);
        return Ok(ascribed(l, expr, tpe, Span::NONE));
    }
    let to_value = l.u.value_class(target).is_some();
    Ok(match (&found, &test) {
        (_, Erased::Unit) => return unsupported(span, "casts of value class values to Unit"),
        (Erased::Prim(a), _) if to_value => match boxed_cast(l, t, expr, *a) {
            Some(t) => t,
            None => return unsupported(span, "casts of primitive values to value classes"),
        },
        (_, Erased::Prim(_)) => {
            let object = Erased::Class("java.lang.Object".into());
            let boxed = adapt(l, expr, &object);
            let object = crate::phases::casts::cast(l, boxed, &object);
            l.explicit_casts.insert(object);
            adapt(l, object, &test)
        }
        (Erased::Class(_), _) if to_value && l.u.value_class(widened).is_some() => {
            return unsupported(span, "casts between value classes");
        }
        _ if to_value => expr,
        _ => {
            l.explicit_casts.insert(t);
            t
        }
    })
}

/// Cast `t` of primitive `expr` of erased letter `c` made a cast of `expr` boxed.
fn boxed_cast(l: &mut Lower, t: TNodeId, expr: TNodeId, c: char) -> Option<TNodeId> {
    let p = l.defs.primitive(c)?;
    let (module, box_method) = (p.module, p.box_method);
    let boxed = converted(l, expr, module, box_method);
    let Tree::TypeApply { fun, .. } = *l.tpd.tree(t) else { unreachable!("a type application") };
    let Tree::Select { name, .. } = *l.tpd.tree(fun) else { unreachable!("a selection") };
    l.tpd.get_mut(fun).tree = Tree::Select { qualifier: boxed, name };
    l.tpd.get_mut(fun).span = l.tpd.span(expr);
    l.explicit_casts.insert(t);
    Some(t)
}

/// `expr.toX` for primitive `expr` of type `tpe` and erased letter `from`, converted to primitive
/// `to`, if `expr`'s class has that method (dotty's `primitiveConversion`).
fn primitive_conversion(l: &mut Lower, expr: TNodeId, tpe: TypeId, from: char, to: char) -> Option<TNodeId> {
    let class = l.class_of(tpe)?;
    let target = crate::definitions::primitive_class_name(to)?;
    let simple = target.rsplit('.').next()?;
    let method = l.u.decl(class, &format!("to{simple}")).ok()?.into_iter().next()?;
    if from == to {
        return None;
    }
    let select = l.select(expr, method, Span::NONE);
    Some(l.positioned(select))
}

/// `{ expr; throw new ClassCastException("Cannot cast to scala.Nothing") }`, at `expr`.
fn throw_class_cast(l: &mut Lower, expr: TNodeId) -> TNodeId {
    let class = l.u.class_ref("java.lang.ClassCastException");
    let class = l.u.class_symbol(class).expect("`ClassCastException`");
    let string = l.u.class_ref("java.lang.String");
    let string = l.u.class_symbol(string);
    let ctors = l.u.decl(class, "<init>").unwrap_or_default();
    let ctor = ctors.into_iter().find(|&c| {
        let info = l.u.info(c);
        matches!(l.u.types().get(info).clone(), Type::Method { params, .. }
            if params.len() == 1 && l.u.class_symbol(params[0].1) == string)
    });
    let ctor = ctor.expect("`ClassCastException(String)`");
    let new = l.new_(class, Span::NONE);
    let message = l.string_literal("Cannot cast to scala.Nothing");
    let error = l.call(new, ctor, &[message]);
    l.tpd.get_mut(error).tpe = l.tpd.get(new).tpe;
    let throw = l.throw_(error);
    let span = l.tpd.span(expr);
    let block = l.block(&[expr], throw, Span::NONE);
    l.with_span(block, span)
}
