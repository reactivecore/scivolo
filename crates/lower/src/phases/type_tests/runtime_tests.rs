//! The test types dotty tests by calls of runtime methods: tuple types (`Tuples.isInstanceOf..`)
//! and arrays of elements that are not of a class (`ScalaRunTime.isArray`).

use crate::Lower;
use crate::phases::adapt::from_start;
use syntax::ast::Span;
use tasty::{Kind, Type, TypeId};
use typer::tpd::TNodeId;

/// `Tuples.isInstanceOfTuple(expr)`, `isInstanceOfNonEmptyTuple` or `isInstanceOfEmptyTuple` at
/// `span` if dealiased type `tpe` is `Tuple`, `NonEmptyTuple`, a `*:` or `EmptyTuple`.
pub fn tuple_test(l: &mut Lower, expr: TNodeId, tpe: TypeId, span: Span) -> Option<TNodeId> {
    let class = l.u.class_symbol(tpe).map(|c| l.u.full_name(c));
    let term = match l.u.types().get(tpe).clone() {
        Type::Sym(_, s) => Some(s),
        Type::TermRef(p, n) => l.u.resolve(p, &n, false),
        _ => None,
    };
    let module = term.filter(|&s| l.u.is_module(s)).map(|s| l.u.full_name(s));
    let name = match (class.as_deref(), module.as_deref()) {
        (_, Some(m)) if m.ends_with(".EmptyTuple") => "isInstanceOfEmptyTuple",
        (Some("scala.Tuple"), _) => "isInstanceOfTuple",
        (Some("scala.NonEmptyTuple" | "scala.*:"), _) => "isInstanceOfNonEmptyTuple",
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

/// Whether dealiased type `tpe` is an array, of arrays, of elements that are not of a class
/// (dotty's `isGenericArrayElement`), tested by `ScalaRunTime.isArray`.
pub fn is_generic_array(l: &mut Lower, tpe: TypeId) -> bool {
    let mut elem = tpe;
    let mut array = false;
    loop {
        let dealiased = l.u.dealias(elem);
        let Type::Applied(tycon, args) = l.u.types().get(dealiased).clone() else { break };
        if l.u.class_symbol(tycon).is_none_or(|c| l.u.full_name(c) != "scala.Array") || args.len() != 1 {
            break;
        }
        (elem, array) = (args[0], true);
    }
    let dealiased = l.u.dealias(elem);
    let class = match *l.u.types().get(dealiased) {
        Type::Sym(_, s) => Some(s),
        Type::Applied(..) | Type::TypeRef(..) => l.u.class_symbol(dealiased),
        _ => None,
    };
    array && class.is_none_or(|c| l.u.symbol(c).kind != Kind::Class)
}
