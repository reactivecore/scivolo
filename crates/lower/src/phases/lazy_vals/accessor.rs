// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/LazyVals.scala
//! The accessor of a lazy value of a class (the accessor half of dotty's `mkThreadSafeDef`).

use super::Lazy;
use crate::Lower;
use crate::definitions::LazyVals;
use crate::phases::adapt::adapt;
use crate::phases::lazy_trees::{block, call, eq, field_ref, if_, local, object, val_def};
use crate::symbols::is_singleton;
use syntax::ast::Span;
use tasty::{Erased, SymbolId, TypeId};
use typer::tpd::{TNodeId, Tree};

/// `{ val result = x$lzy; if result.isInstanceOf[T] then result else if result eq NullValue then
/// null else x$lzyINIT() }`, each branch adapted to the value's type `info`; the test is
/// `result != null && !result.isInstanceOf[LazyValControlState]` where `T` includes the states.
pub fn accessor_body(l: &mut Lower, lazy: &Lazy, info: TypeId, defs: LazyVals) -> TNodeId {
    let object = object(l);
    let result = local(l, lazy.accessor, "result", object, false);
    let field = field_ref(l, lazy);
    let def = val_def(l, result, field);
    let erased = l.u.erased_method(lazy.accessor).1;
    let info = if l.u.erase(info) == Erased::Unit { crate::phases::casts::type_of(l, &erased) } else { info };
    let cond = if holds_control_states(l, &erased, defs.control_state) {
        let read = l.ident(result, Span::NONE);
        let null = l.null_literal(Span::NONE);
        let set = call(l, read, l.defs.ne, &[null]);
        let read = l.ident(result, Span::NONE);
        let control = crate::definitions::type_ref(l.u, defs.control_state);
        let is_control = l.is_instance(read, control);
        let not_control = call(l, is_control, defs.not, &[]);
        call(l, set, l.defs.and, &[not_control])
    } else {
        let test_type = boxed_type(l, &erased, info);
        let read = l.ident(result, Span::NONE);
        l.is_instance(read, test_type)
    };
    let read = l.ident(result, Span::NONE);
    let value = conform(l, read, &erased, info);
    let read = l.ident(result, Span::NONE);
    let null_value = l.ident(defs.null_value, Span::NONE);
    let is_null = eq(l, read, null_value);
    let null = l.null_literal(Span::NONE);
    if erased != Erased::Null {
        l.tpd.get_mut(null).tpe = Some(object);
    }
    let null = conform(l, null, &erased, info);
    if is_singleton(l.u, info) {
        l.tpd.get_mut(null).tpe = Some(info);
    }
    let this = l.this(lazy.class, Span::NONE);
    let init = call(l, this, lazy.init, &[]);
    let init = conform(l, init, &erased, info);
    let inner = if_(l, is_null, null, init, info);
    let outer = if_(l, cond, value, inner, info);
    block(l, &[def], outer)
}

/// `t` adapted to erased type `erased` of type `tpe`, cast to `Null` or `Nothing` unless it is
/// `null` where `Null` is expected (dotty's `ensureConforms` after erasure).
fn conform(l: &mut Lower, t: TNodeId, erased: &Erased, tpe: TypeId) -> TNodeId {
    let null = matches!(l.tpd.tree(t), Tree::Literal { value: syntax::ast::constant::Constant::Null });
    match erased {
        Erased::Null if null => t,
        Erased::Null | Erased::Nothing => {
            let fun = l.select(t, l.defs.as_instance_of, Span::NONE);
            let tpt = l.type_tree(tpe, Span::NONE);
            let cast = l.build().type_apply(fun, &[tpt], tpe, Span::NONE);
            l.positioned(cast)
        }
        _ => adapt(l, t, erased),
    }
}

/// Whether the values of erased type `erased` include those of `control`, `LazyValControlState`.
fn holds_control_states(l: &mut Lower, erased: &Erased, control: SymbolId) -> bool {
    let Erased::Class(name) = erased else { return false };
    crate::phases::casts::class_named(l, name).is_some_and(|c| l.u.derives_from(control, c))
}

/// The class `T` of type `info` erasing to `erased` as a reference: its box for a primitive.
fn boxed_type(l: &mut Lower, erased: &Erased, info: TypeId) -> TypeId {
    match erased {
        Erased::Prim(c) => l.u.class_ref(crate::definitions::boxed_class(*c).expect("a primitive's box")),
        _ => info,
    }
}
