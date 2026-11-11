// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
//   compiler/src/dotty/tools/dotc/transform/TypeTestsCasts.scala
//! The test types dotty tests by calls of runtime methods: tuple types (`Tuples.isInstanceOf..`)
//! and arrays of elements that are not of a class (`ScalaRunTime.isArray`).

use crate::definitions::Known;
use crate::phases::adapt::from_start;
use crate::phases::first_transform::is_pure;
use crate::{Lower, Result, unsupported};
use names::nme;
use syntax::ast::Span;
use tasty::{Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// `Tuples.isInstanceOfTuple(expr)`, `isInstanceOfNonEmptyTuple` or `isInstanceOfEmptyTuple` at
/// `span` if dealiased type `tpe` refers to `Tuple`, `NonEmptyTuple` or `EmptyTuple`, or applies
/// `*:`.
pub fn tuple_test(l: &mut Lower, expr: TNodeId, tpe: TypeId, span: Span) -> Option<TNodeId> {
    let (tycon, applied) = l.u.applied(tpe).map_or((tpe, false), |(tycon, _)| (tycon, true));
    let s = match *l.u.ty(tycon) {
        Type::Sym(_, s) => Some(s),
        Type::TypeRef(..) | Type::TermRef(..) => l.u.resolve_ref(tycon),
        _ => None,
    };
    let name = match (s.map(|s| l.u.full_name(s)).as_deref(), applied) {
        (Some("scala.Tuple$package$.EmptyTuple"), false) if s.is_some_and(|s| l.u.is_module(s)) => {
            "isInstanceOfEmptyTuple"
        }
        (Some("scala.Tuple"), false) => "isInstanceOfTuple",
        (Some("scala.NonEmptyTuple"), false) | (Some("scala.*:"), true) => "isInstanceOfNonEmptyTuple",
        _ => return None,
    };
    let runtime = l.u.package("scala.runtime")?;
    let tuples = l.u.decl(runtime, "Tuples").ok()?.into_iter().find(|&s| l.u.is_module(s))?;
    let class = l.u.module_class(tuples)?;
    let method = *l.u.decl(class, name).ok()?.first()?;
    let module_ref = l.ident(tuples, from_start(l.tpd.span(expr)));
    let fun = l.select(module_ref, method, Span::NONE);
    let boolean = l.u.class_ref(tasty::defn::BOOLEAN);
    Some(l.apply(fun, &[expr], boolean, from_start(span)))
}

/// `ScalaRunTime.isArray(expr, 1)` at `span` if dealiased type `tpe` is an array of elements that
/// no JVM array holds all of (dotty's `isGenericArrayElement`); for `n` dimensions, a name `x`
/// tested for its own type and `ScalaRunTime.isArray(x, n)`.
pub fn array_test(l: &mut Lower, expr: TNodeId, tpe: TypeId, span: Span) -> Result<Option<TNodeId>> {
    let mut elem = tpe;
    let mut dims = 0;
    loop {
        let dealiased = l.u.dealias(elem);
        let Some((tycon, args)) = l.u.applied(dealiased).map(|(c, a)| (c, a.to_vec())) else { break };
        if !l.u.class_symbol(tycon).is_some_and(|c| l.is_known(c, Known::Array)) || args.len() != 1 {
            break;
        }
        (elem, dims) = (args[0], dims + 1);
    }
    if dims == 0 || !l.u.is_generic(elem) {
        return Ok(None);
    }
    if dims == 1 {
        return is_array(l, expr, 1, span).map(Some);
    }
    let (&Tree::Ident { name }, true) = (l.tpd.tree(expr), is_pure(l, expr)) else {
        return unsupported(span, "type tests of generic arrays of arrays of paths other than names");
    };
    let n = l.tpd.get(expr);
    let (expr_span, tpe, sym) = (n.span, n.tpe, n.sym);
    let other = l.tpd.node(Tree::Ident { name }, expr_span, tpe, sym);
    let own = super::type_test(l, expr, l.tpd.tpe(expr), false, span)?;
    let array = is_array(l, other, dims, span)?;
    let boolean = l.u.class_ref(tasty::defn::BOOLEAN);
    let boolean = l.u.class_symbol(boolean).expect("`Boolean`");
    let and = l.u.decl(boolean, nme::ZAND).ok().and_then(|ms| ms.into_iter().next()).expect("`Boolean.&&`");
    Ok(Some(l.call(own, and, &[array])))
}

/// `ScalaRunTime.isArray(expr, dims)` at `span`.
fn is_array(l: &mut Lower, expr: TNodeId, dims: i32, span: Span) -> Result<TNodeId> {
    let module = l.scala_run_time()?;
    let class = l.u.module_class(module);
    let found = class.and_then(|c| l.u.decl(c, nme::IS_ARRAY).ok()).and_then(|d| d.first().copied());
    let Some(method) = found else { return unsupported(span, "`ScalaRunTime.isArray` not found") };
    let module_ref = l.ident(module, from_start(l.tpd.span(expr)));
    let fun = l.select(module_ref, method, Span::NONE);
    let dims = l.int_literal(dims);
    let boolean = l.u.class_ref(tasty::defn::BOOLEAN);
    Ok(l.apply(fun, &[expr, dims], boolean, from_start(span)))
}
