//! The test types dotty tests by calls of runtime methods: tuple types (`Tuples.isInstanceOf..`)
//! and arrays of elements that are not of a class (`ScalaRunTime.isArray`).

use crate::phases::adapt::from_start;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::{Kind, Type, TypeId};
use typer::tpd::TNodeId;

/// `Tuples.isInstanceOfTuple(expr)`, `isInstanceOfNonEmptyTuple` or `isInstanceOfEmptyTuple` at
/// `span` if dealiased type `tpe` refers to `Tuple`, `NonEmptyTuple` or `EmptyTuple`, or applies
/// `*:`.
pub fn tuple_test(l: &mut Lower, expr: TNodeId, tpe: TypeId, span: Span) -> Option<TNodeId> {
    let (tycon, applied) = match *l.u.types().get(tpe) {
        Type::Applied(tycon, _) => (tycon, true),
        _ => (tpe, false),
    };
    let s = match *l.u.types().get(tycon) {
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
    let boolean = l.u.class_ref("scala.Boolean");
    Some(l.apply(fun, &[expr], boolean, from_start(span)))
}

/// `ScalaRunTime.isArray(expr, 1)` at `span` if dealiased type `tpe` is an array of elements that
/// are not of a class (dotty's `isGenericArrayElement`).
pub fn array_test(l: &mut Lower, expr: TNodeId, tpe: TypeId, span: Span) -> Result<Option<TNodeId>> {
    let mut elem = tpe;
    let mut dims = 0;
    loop {
        let dealiased = l.u.dealias(elem);
        let Type::Applied(tycon, args) = l.u.types().get(dealiased).clone() else { break };
        if l.u.class_symbol(tycon).is_none_or(|c| l.u.full_name(c) != "scala.Array") || args.len() != 1 {
            break;
        }
        (elem, dims) = (args[0], dims + 1);
    }
    let dealiased = l.u.dealias(elem);
    let class = match *l.u.types().get(dealiased) {
        Type::Sym(_, s) => Some(s),
        Type::Applied(..) | Type::TypeRef(..) => l.u.class_symbol(dealiased),
        _ => None,
    };
    if dims == 0 || class.is_some_and(|c| l.u.symbol(c).kind == Kind::Class) {
        return Ok(None);
    }
    if dims > 1 {
        return unsupported(span, "type tests of generic arrays of arrays");
    }
    let module = l.scala_run_time()?;
    let class = l.u.module_class(module);
    let found = class.and_then(|c| l.u.decl(c, "isArray").ok()).and_then(|d| d.first().copied());
    let Some(method) = found else { return unsupported(span, "`ScalaRunTime.isArray` not found") };
    let module_ref = l.ident(module, from_start(l.tpd.span(expr)));
    let fun = l.select(module_ref, method, Span::NONE);
    let one = l.int_literal(1);
    let boolean = l.u.class_ref("scala.Boolean");
    Ok(Some(l.apply(fun, &[expr, one], boolean, from_start(span))))
}
