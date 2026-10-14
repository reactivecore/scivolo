//! The boxing and unboxing of value classes (dotty's `Erasure.Boxing.box` and `unbox` of an
//! `ErasedValueType`): `new C(v)` and `v.value()`, null-checked unless `v` is statically a `C`.

use super::{adapt, applied_to_none, from_start};
use crate::Lower;
use crate::definitions::type_ref;
use crate::phases::casts::{cast, class_named, conforms, type_of};
use crate::phases::value_classes::eval_once_as;
use crate::phases::vc_elide::elided;
use syntax::ast::Span;
use tasty::{Erased, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// `adapt` of `t` of erased type `from`, value classes boxed into references and unboxed from
/// them first.
pub fn adapt_from(l: &mut Lower, t: TNodeId, from: Erased, pt: &Erased) -> TNodeId {
    let reference = |e: &Erased| matches!(e, Erased::Class(_) | Erased::Array(_));
    match (from, pt) {
        (from @ Erased::Value(..), pt) if from == *pt => t,
        (Erased::Value(class, u), Erased::Value(_, v)) => {
            let c = adapt_from(l, t, *u, v);
            if c == t {
                return c;
            }
            let class = class_named(l, &class).expect("a value class");
            let tpe = type_ref(l.u, class);
            let span = l.tpd.span(c);
            super::ascribed(l, c, tpe, span)
        }
        (Erased::Value(_, u), pt) if *u == *pt => t,
        (Erased::Value(class, u), pt) if reference(pt) => {
            let boxed = boxed(l, t, &class, &u);
            adapt(l, boxed, pt)
        }
        (Erased::Value(_, u), pt) => adapt_from(l, t, *u, pt),
        (Erased::Nothing, Erased::Value(..)) => t,
        (from @ Erased::Prim(_), Erased::Value(_, u)) if from == **u => t,
        (from, Erased::Value(class, u)) => unboxed(l, t, &from, class, u),
        (from, pt) => super::adapt_plain(l, t, &from, pt),
    }
}

/// `new C(t)` for value `t` of value class `class`, `t` ascribed its erased `underlying` type.
fn boxed(l: &mut Lower, t: TNodeId, class: &str, underlying: &Erased) -> TNodeId {
    let class = class_named(l, class).expect("a value class");
    let span = from_start(l.tpd.span(t));
    let value_type = type_of(l, underlying);
    let tpt = l.type_tree(value_type, Span::NONE);
    let value = l.tpd.node(Tree::Typed { expr: t, tpt }, l.tpd.span(t), Some(value_type), None);
    let new = l.new_(class, span);
    let ctor = l.u.decl(class, "<init>").unwrap_or_default();
    let ctor = *ctor.first().expect("a value class's constructor");
    let fun = l.select(new, ctor, span);
    let tpe = type_ref(l.u, class);
    l.apply(fun, &[value], tpe, span)
}

/// `t`, of erased type `from`, as the value of value class `class` with erased underlying type
/// `underlying`.
fn unboxed(l: &mut Lower, t: TNodeId, from: &Erased, class: &str, underlying: &Erased) -> TNodeId {
    let boxed = Erased::Class(class.to_owned());
    if *from == Erased::Null {
        return null_as(l, t, underlying);
    }
    if conforms(l, from, &boxed) || matches!(from, Erased::Nothing) {
        let span = from_start(l.tpd.span(t));
        let value = value_of(l, t, class, underlying);
        return cast_generic(l, value, class, underlying, span);
    }
    let local = matches!(l.tpd.tree(t), Tree::Ident { .. })
        && l.tpd.sym(t).is_some_and(|s| !l.u.symbol(s).owner.is_some_and(|o| l.u.is_class(o)));
    let (value, def) = if local {
        (t, None)
    } else {
        let owner = l.erasure_owner.expect("the method an unboxed value is evaluated in");
        let tpe = type_of(l, from);
        let (ev, def) = eval_once_as(l, t, owner, tpe);
        (l.ident(ev, Span::NONE), Some(def))
    };
    let null = l.null_literal(Span::NONE);
    let eq = l.select(value, l.defs.patmat.eq, Span::NONE);
    let eq = l.positioned(eq);
    let boolean = l.u.class_ref("scala.Boolean");
    let cond = l.apply(eq, &[null], boolean, Span::NONE);
    let cond = l.positioned(cond);
    let null = l.null_literal(Span::NONE);
    let thenp = null_as(l, null, underlying);
    let again = l.copy_ident(value);
    if def.is_none() {
        l.tpd.get_mut(again).span = l.tpd.span(value);
    }
    let elsep = value_of(l, again, class, underlying);
    let tpe = l.tpd.tpe(elsep);
    let tree = Tree::If { cond, thenp, elsep, inline: false };
    let node = l.tpd.node(tree, Span::NONE, Some(tpe), None);
    let node = l.positioned(node);
    let node = match def {
        Some(def) => {
            let block = l.block(&[def], node, Span::NONE);
            l.positioned(block)
        }
        None => node,
    };
    let span = l.tpd.span(node);
    cast_generic(l, node, class, underlying, span)
}

/// Whether the accessor of value class `class` returns a type other than its reference
/// `underlying` erasure, a type parameter's.
fn generic(l: &mut Lower, class: &str, underlying: &Erased) -> bool {
    let class = class_named(l, class).expect("a value class");
    let value = l.u.underlying(class).expect("a value class's value");
    let reference = matches!(underlying, Erased::Class(_) | Erased::Array(_));
    reference && l.u.erased_method_value(value).1 != *underlying
}

/// `t`, the unboxed value of value class `class`, cast to its erased `underlying` type and
/// ascribed `class` at `span` if the accessor is `generic` (dotty's `cast` after `unbox`).
fn cast_generic(l: &mut Lower, t: TNodeId, class: &str, underlying: &Erased, span: Span) -> TNodeId {
    if !generic(l, class, underlying) {
        return t;
    }
    let c = cast(l, t, underlying);
    l.tpd.get_mut(c).span = span;
    l.explicit_casts.insert(c);
    let class = class_named(l, class).expect("a value class");
    let tpe = type_ref(l.u, class);
    super::ascribed(l, c, tpe, span)
}

/// `null` as a value of erased type `to`: unboxed for a primitive, cast for an array.
fn null_as(l: &mut Lower, null: TNodeId, to: &Erased) -> TNodeId {
    match to {
        Erased::Prim(c) => match l.defs.primitive(*c) {
            Some(p) => {
                let (module, unbox_method) = (p.module, p.unbox_method);
                super::converted(l, null, module, unbox_method)
            }
            None => null,
        },
        Erased::Array(_) => {
            let c = cast(l, null, to);
            l.null_casts.insert(c);
            c
        }
        _ => null,
    }
}

/// `t.v()` for the accessor `v` of the underlying value of value class `class`, `t` cast to
/// `class` of erased `underlying` type; a reference value ascribed `class`, at the call's span,
/// so it stays unboxed, unless `generic`.
fn value_of(l: &mut Lower, t: TNodeId, name: &str, underlying: &Erased) -> TNodeId {
    let boxed = Erased::Class(name.to_owned());
    let t = adapt(l, t, &boxed);
    let class: SymbolId = class_named(l, name).expect("a value class");
    let value = l.u.underlying(class).expect("a value class's value");
    let span = from_start(l.tpd.span(t));
    let sel = l.select(t, value, span);
    let call = applied_to_none(l, sel);
    let primitive = matches!(super::erased_type(l, call), Erased::Prim(_));
    let e = elided(l, call);
    if primitive || generic(l, name, underlying) {
        return e;
    }
    let tpe = type_ref(l.u, class);
    let span = l.tpd.span(e);
    super::ascribed(l, e, tpe, span)
}
