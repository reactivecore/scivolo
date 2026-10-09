//! Type tests and casts as dotty's `TypeTestsCasts` erases them: `x.isInstanceOf[T]` as a null
//! check if it holds statically in a match, else as a test of `T`'s erased class, the box class
//! for a primitive; `x.asInstanceOf[T]` of a reference to a primitive as unboxing.

use super::adapt::{adapt, erased_type};
use super::casts::type_of;
use crate::symbols::{is_module, is_static};
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::{Erased, Type};
use typer::tpd::{TNodeId, Tree};

/// Whether `t` is a type test or cast `x.isInstanceOf[T]`, `x.$isInstanceOf[T]` or
/// `x.asInstanceOf[T]`.
pub fn is_test_or_cast(l: &Lower, t: TNodeId) -> bool {
    let Tree::TypeApply { fun, .. } = *l.tpd.tree(t) else { return false };
    let p = l.defs.patmat;
    l.tpd.sym(fun).is_some_and(|s| [p.is_instance_of, p.type_test, l.defs.as_instance_of].contains(&s))
}

/// Makes type test or cast `t` its erased form, at `t`'s span.
pub fn intercept(l: &mut Lower, t: TNodeId) -> Result<()> {
    let Tree::TypeApply { fun, args } = *l.tpd.tree(t) else { unreachable!("a type application") };
    let span = l.tpd.span(t);
    let Tree::Select { qualifier: expr, .. } = *l.tpd.tree(fun) else {
        return unsupported(span, "type tests and casts without a receiver");
    };
    let target = l.tpd.tpe(l.tpd.items(args)[0]);
    let new = if l.tpd.sym(fun) == Some(l.defs.as_instance_of) {
        cast(l, t, expr, target)?
    } else {
        let in_match = l.tpd.sym(fun) == Some(l.defs.patmat.type_test);
        type_test(l, expr, target, in_match, span)?
    };
    if new != t {
        l.copy_node(t, new);
        l.tpd.get_mut(t).span = span;
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
    if let Type::Sym(_, s) = *l.u.types().get(target)
        && is_module(l.u, s)
        && !is_static(l.u, s)
    {
        return unsupported(span, "type tests of objects nested in classes");
    }
    let widened = l.u.widen(target);
    let dealiased = l.u.dealias(widened);
    if matches!(l.u.types().get(dealiased), Type::Or(..) | Type::And(..)) {
        return unsupported(span, "type tests of unions or intersections");
    }
    let tuple = l.u.class_symbol(dealiased).map(|c| l.u.full_name(c));
    if tuple.is_some_and(|n| {
        ["scala.Tuple", "scala.NonEmptyTuple", "scala.*:", "scala.EmptyTuple"].contains(&n.as_str())
    }) {
        return unsupported(span, "type tests of tuple types");
    }
    let test = l.u.erase(target);
    if !matches!(test, Erased::Class(_) | Erased::Prim(_)) {
        return unsupported(span, "type tests of arrays, singletons or Unit");
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
    let found = erased_type(l, expr);
    if !matches!(found, Erased::Class(_) | Erased::Array(_)) {
        return unsupported(span, "type tests of primitive values or of null");
    }
    let expr_tpe = l.tpd.tpe(expr);
    let test_tpe = type_of(l, &test);
    if in_match && l.u.is_subtype(expr_tpe, test_tpe) {
        if is_not_null(l, expr_tpe) {
            return Ok(constant(l, expr, true));
        }
        let null = l.null_literal(Span::NONE);
        let ne = l.select(expr, l.defs.patmat.ne, Span::NONE);
        let ne = l.positioned(ne);
        let boolean = l.u.class_ref("scala.Boolean");
        let test = l.apply(ne, &[null], boolean, Span::NONE);
        return Ok(l.positioned(test));
    }
    if !super::sensical::sensical(l, expr_tpe, &test, unboxed) {
        return Ok(constant(l, expr, false));
    }
    if let Erased::Prim(c) = test {
        let boxed = boxed_class(c).expect("a primitive's box class");
        return is_instance_of(l, expr, Erased::Class(boxed.into()), &test, in_match, span);
    }
    let tested = type_of(l, &test);
    let fun = l.select(expr, l.defs.patmat.is_instance_of, l.tpd.span(expr));
    let tpt = l.type_tree(tested, Span::NONE);
    let args = l.tpd.list(&[tpt]);
    let boolean = l.u.class_ref("scala.Boolean");
    Ok(l.tpd.node(Tree::TypeApply { fun, args }, span, Some(boolean), Some(l.defs.patmat.is_instance_of)))
}

/// Cast `t` of `expr` to `target`: unboxed if `expr` is a reference and `target` primitive,
/// `t` if both are references or the same primitive.
fn cast(l: &mut Lower, t: TNodeId, expr: TNodeId, target: tasty::TypeId) -> Result<TNodeId> {
    let (from, to) = (erased_type(l, expr), l.u.erase(target));
    match (&from, &to) {
        (Erased::Class(_) | Erased::Array(_) | Erased::Null, Erased::Nothing) => {
            Ok(throw_class_cast(l, expr))
        }
        (Erased::Prim(a), Erased::Prim(b)) if a == b => Ok(expr),
        (Erased::Class(_) | Erased::Array(_), Erased::Prim(_)) => Ok(adapt(l, expr, &to)),
        (Erased::Prim(_), _) | (_, Erased::Prim(_)) => {
            unsupported(l.tpd.span(t), "casts between primitive values or to references")
        }
        _ => Ok(t),
    }
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

/// Whether a value of type `tpe` is never null: a constant other than `null` (dotty's
/// `isNotNull`).
fn is_not_null(l: &mut Lower, tpe: tasty::TypeId) -> bool {
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

/// Boolean `value`, after `expr` unless it is a literal, identifier or `this` (dotty's
/// `constant`).
fn constant(l: &mut Lower, expr: TNodeId, value: bool) -> TNodeId {
    let span = l.tpd.span(expr);
    let tpe = l
        .u
        .intern_type(Type::Constant(if value { tasty::tags::TRUEconst } else { tasty::tags::FALSEconst }, 0));
    let lit = l.tpd.node(Tree::Literal { value: Constant::Boolean(value) }, Span::NONE, Some(tpe), None);
    match l.tpd.tree(expr) {
        Tree::Literal { .. } | Tree::Ident { .. } | Tree::This { .. } => lit,
        _ => l.block(&[expr], lit, span),
    }
}

/// The box class of the primitive of erased letter `c`.
pub fn boxed_class(c: char) -> Option<&'static str> {
    Some(match c {
        'z' => "java.lang.Boolean",
        'c' => "java.lang.Character",
        'b' => "java.lang.Byte",
        's' => "java.lang.Short",
        'i' => "java.lang.Integer",
        'j' => "java.lang.Long",
        'f' => "java.lang.Float",
        'd' => "java.lang.Double",
        _ => return None,
    })
}
