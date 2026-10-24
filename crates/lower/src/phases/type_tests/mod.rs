// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/TypeTestsCasts.scala
//! Type tests and casts as dotty's `TypeTestsCasts` erases them: `x.isInstanceOf[T]` as a null
//! check if it holds statically in a match, a constant for a primitive value, else as a test of
//! `T`'s erased class, a value class itself, the box class for a primitive; casts in
//! `as_instance_of.rs`.

mod as_instance_of;
mod combined;
mod runtime_tests;
pub(super) mod singletons;

use super::adapt::erased_type;
use super::casts::type_of;
use crate::definitions::boxed_class;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::{Erased, Type};
use typer::tpd::{TNodeId, Tree};

/// Whether `t` is a type test or cast `x.isInstanceOf[T]`, `x.$isInstanceOf$[T]`,
/// `x.asInstanceOf[T]` or `x.$asInstanceOf$[T]`.
pub fn is_test_or_cast(l: &Lower, t: TNodeId) -> bool {
    let Tree::TypeApply { fun, .. } = *l.tpd.tree(t) else { return false };
    let d = &l.defs;
    l.tpd
        .sym(fun)
        .is_some_and(|s| [d.is_instance_of, d.type_test, d.as_instance_of, d.type_cast].contains(&s))
}

/// Numbers the values type tests evaluate once in the order of the unit's trees.
pub fn renumber_tested_values(l: &mut Lower) {
    combined::renumber(l);
}

/// Makes type test or cast `t` its erased form, at `t`'s span; `$asInstanceOf$` as
/// `asInstanceOf`.
pub fn intercept(l: &mut Lower, t: TNodeId) -> Result<()> {
    let Tree::TypeApply { fun, args } = *l.tpd.tree(t) else { unreachable!("a type application") };
    if l.tpd.sym(fun) == Some(l.defs.type_cast) {
        l.tpd.get_mut(fun).sym = Some(l.defs.as_instance_of);
    }
    let span = l.tpd.span(t);
    let Tree::Select { qualifier: expr, .. } = *l.tpd.tree(fun) else {
        return unsupported(span, "type tests and casts without a receiver");
    };
    let tpt = l.tpd.items(args)[0];
    let target = l.tpd.tpe(tpt);
    let new = if l.tpd.sym(fun) == Some(l.defs.as_instance_of) {
        as_instance_of::cast(l, t, expr, tpt)?
    } else {
        let in_match = l.tpd.sym(fun) == Some(l.defs.type_test);
        type_test(l, expr, target, in_match, span)?
    };
    if new != t {
        let typed = matches!(l.tpd.tree(new), Tree::Typed { .. });
        l.copy_node(t, new);
        l.tpd.get_mut(t).span = if typed { super::adapt::from_start(span) } else { span };
    }
    Ok(())
}

/// `expr.isInstanceOf[target]` erased (dotty's `transformTypeTest`).
fn type_test(
    l: &mut Lower,
    expr: TNodeId,
    target: tasty::TypeId,
    in_match: bool,
    span: Span,
) -> Result<TNodeId> {
    if let Some(what) = super::outer_tests::unsupported_test(l, target, in_match) {
        return unsupported(span, what);
    }
    let dealiased = l.u.dealias(target);
    if let Type::Or(a, b, _) | Type::And(a, b) = *l.u.types().get(dealiased) {
        let or = matches!(l.u.types().get(dealiased), Type::Or(..));
        return combined::test(l, expr, (a, b), or, in_match, span);
    }
    if let Some(test) = runtime_tests::array_test(l, expr, dealiased, span)? {
        return Ok(test);
    }
    if let Some(test) = runtime_tests::tuple_test(l, expr, dealiased, span) {
        return Ok(test);
    }
    if singletons::has_value_class_this(l, dealiased) {
        return unsupported(span, "type tests of this.type of value classes");
    }
    if let Some(test) = singletons::is_instance(l, expr, dealiased, None) {
        return Ok(l.with_span(test, span));
    }
    let widened = l.u.widen(target);
    let dealiased = l.u.dealias(widened);
    if matches!(l.u.types().get(dealiased), Type::Or(..) | Type::And(..)) {
        return unsupported(span, "type tests of unions or intersections of singletons");
    }
    let test = match l.u.erase_boxed(target) {
        Erased::Unit => Erased::Class("scala.runtime.BoxedUnit".into()),
        e => e,
    };
    if !matches!(test, Erased::Class(_) | Erased::Prim(_) | Erased::Array(_)) {
        return unsupported(span, "type tests of singletons, Null or Nothing");
    }
    is_instance_of(l, expr, test.clone(), &test, in_match, span)
}

/// `expr` tested for erased class `test`, `unboxed` the primitive tested for if `test` is its box
/// class (dotty's `transformIsInstanceOf`).
fn is_instance_of(
    l: &mut Lower,
    expr: TNodeId,
    test: Erased,
    unboxed: &Erased,
    in_match: bool,
    span: Span,
) -> Result<TNodeId> {
    let widened = l.u.widen(l.tpd.tpe(expr));
    let found = match l.u.erase_boxed(widened) {
        Erased::Prim(c) => Some(c),
        Erased::Class(_) | Erased::Array(_)
            if matches!(erased_type(l, expr), Erased::Class(_) | Erased::Array(_) | Erased::Value(..)) =>
        {
            None
        }
        Erased::Null if erased_type(l, expr) == Erased::Null => None,
        _ => return unsupported(span, "type tests of primitive values or of null"),
    };
    let expr_tpe = l.tpd.tpe(expr);
    let test_tpe = type_of(l, &test);
    let boxed = found.and_then(boxed_class).map(|b| l.u.class_ref(b));
    let array = matches!(test, Erased::Array(_));
    if in_match && !array && l.u.is_subtype(boxed.unwrap_or(expr_tpe), test_tpe) {
        if found.is_some() {
            return unsupported(span, "type tests of primitive values");
        }
        if is_not_null(l, expr_tpe) {
            return Ok(constant(l, expr, true));
        }
        let null = l.null_literal(Span::NONE);
        let ne = l.select(expr, l.defs.ne, Span::NONE);
        let ne = l.positioned(ne);
        let boolean = l.u.class_ref("scala.Boolean");
        let test = l.apply(ne, &[null], boolean, Span::NONE);
        return Ok(l.positioned(test));
    }
    if !super::sensical::sensical(l, expr_tpe, &test, unboxed) {
        return Ok(constant(l, expr, false));
    }
    if let (Erased::Prim(c), Some(f)) = (&test, found) {
        return Ok(constant(l, expr, *c == f));
    }
    if found.is_some() && !matches!(test, Erased::Class(_)) {
        return unsupported(span, "type tests of primitive values");
    }
    if let Erased::Prim(c) = test {
        let boxed = boxed_class(c).expect("a primitive's box class");
        return is_instance_of(l, expr, Erased::Class(boxed.into()), &test, in_match, span);
    }
    let tested = type_of(l, &test);
    let fun = l.select(expr, l.defs.is_instance_of, l.tpd.span(expr));
    let tpt = l.type_tree(tested, Span::NONE);
    let args = l.tpd.list(&[tpt]);
    let boolean = l.u.class_ref("scala.Boolean");
    Ok(l.tpd.node(Tree::TypeApply { fun, args }, span, Some(boolean), Some(l.defs.is_instance_of)))
}

/// Whether a value of type `tpe` is never null: a constant other than `null` or of a value class
/// type (dotty's `isNotNull`).
fn is_not_null(l: &mut Lower, tpe: tasty::TypeId) -> bool {
    let widened = l.u.widen(tpe);
    if l.u.value_class(widened).is_some() {
        return true;
    }
    let tpe = match *l.u.types().get(tpe) {
        Type::Sym(_, s) if l.u.symbol(s).kind == tasty::Kind::Val => l.u.info(s),
        _ => tpe,
    };
    match *l.u.types().get(tpe) {
        Type::Constant(tag, _) => tag != tasty::tags::NULLconst,
        Type::Str(_) => true,
        _ => false,
    }
}

/// Boolean `value`, after `expr` unless it is a literal or a pure path (dotty's `constant`).
fn constant(l: &mut Lower, expr: TNodeId, value: bool) -> TNodeId {
    let span = l.tpd.span(expr);
    let lit = l.boolean_literal(value);
    let literal = matches!(l.tpd.tree(expr), Tree::Literal { .. } | Tree::ClassLiteral { .. });
    if literal || super::first_transform::is_pure(l, expr) {
        return l.with_span(lit, span);
    }
    let block = l.block(&[expr], lit, Span::NONE);
    let block = l.positioned(block);
    l.with_span(block, span)
}
