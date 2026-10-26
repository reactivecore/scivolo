// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//   compiler/src/dotty/tools/dotc/transform/ElimRepeated.scala
//   compiler/src/dotty/tools/dotc/transform/SeqLiterals.scala
//! Arrays wrapped as Scala sequences by `ScalaRunTime.wrapXArray`: those passed to repeated
//! parameters, sequence literals and arrays ascribed a repeated type outside arguments.

use super::repeated::is_class;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::{Kind, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// The classes whose arrays `ScalaRunTime` wraps by their own method.
const PRIMITIVES: [&str; 9] = ["Boolean", "Byte", "Char", "Short", "Int", "Long", "Float", "Double", "Unit"];

/// The classes without instances of their own at run time, wrapped generically.
const NOT_RUNTIME: [&str; 6] =
    ["scala.Any", "scala.AnyVal", "scala.Matchable", "scala.Nothing", "scala.Null", "scala.Singleton"];

/// `ScalaRunTime.wrapXArray(array)` for `t`, `array: <repeated>[T]` outside an argument, as an
/// inline method's parameter binds it.
pub fn repeated_array(l: &mut Lower, t: TNodeId) -> Result<Option<TNodeId>> {
    let Tree::Typed { expr, tpt } = *l.tpd.tree(t) else { return Ok(None) };
    let ascribed = l.tpd.tpe(tpt);
    let tpe = l.u.widen(l.tpd.tpe(expr));
    if !is_class(l, ascribed, "scala.<repeated>") || !is_class(l, tpe, "scala.Array") {
        return Ok(None);
    }
    let Type::Applied(_, ref elems) = *l.u.types().get(ascribed) else { return Ok(None) };
    let elem = elems[0];
    let span = l.tpd.span(t);
    wrap_array(l, expr, elem, ascribed, span).map(Some)
}

/// Sequence literal `t` of `elems` outside a repeated argument, of a Scala sequence type, made
/// its elements' array wrapped (dotty's `SeqLiterals`).
pub fn seq_value(l: &mut Lower, t: TNodeId, elems: typer::tpd::List, elemtpt: TNodeId) -> Result<()> {
    let Some(tpe) = l.tpd.get(t).tpe else { return Ok(()) };
    if is_class(l, tpe, "scala.Array") {
        return Ok(());
    }
    let (elem, span) = (l.tpd.tpe(elemtpt), l.tpd.span(t));
    let array = array_of(l, elem);
    let seq = l.tpd.node(Tree::SeqLiteral { elems, elemtpt }, span, Some(array), None);
    let wrapped = wrap_array(l, seq, elem, tpe, span)?;
    l.tpd.get_mut(t).tree = l.tpd.tree(wrapped).clone();
    Ok(())
}

/// `ScalaRunTime.wrapXArray(array)` of type `tpe`, for elements of type `elem` (dotty's
/// `wrapArray`), at `span`.
pub fn wrap_array(l: &mut Lower, array: TNodeId, elem: TypeId, tpe: TypeId, span: Span) -> Result<TNodeId> {
    let wrap = wrapper(l, elem);
    let module = l.scala_run_time()?;
    let class = l.u.module_class(module).expect("ScalaRunTime's class");
    let Some(&wrap) = l.u.decl(class, &wrap).unwrap_or_default().first() else {
        return unsupported(l.tpd.span(array), format!("ScalaRunTime.{wrap} missing from the library"));
    };
    let qual = l.ident(module, span);
    let fun = l.select(qual, wrap, span);
    Ok(l.apply(fun, &[array], tpe, span))
}

pub fn array_of(l: &mut Lower, elem: TypeId) -> TypeId {
    let array = l.u.class_ref("scala.Array");
    l.u.intern_type(Type::Applied(array, vec![elem]))
}

/// The `ScalaRunTime` method wrapping an array of `elem` (dotty's `wrapArrayMethodName`).
fn wrapper(l: &mut Lower, elem: TypeId) -> String {
    let elem = l.u.dealias(elem);
    let Some(class) = l.u.class_symbol(elem).filter(|&c| l.u.symbol(c).kind == Kind::Class) else {
        return "genericWrapArray".into();
    };
    let name = l.u.full_name(class);
    if let Some(p) = name.strip_prefix("scala.").filter(|p| PRIMITIVES.contains(p)) {
        return format!("wrap{p}Array");
    }
    let value_class = l.u.builtins().any_val.is_some_and(|v| l.u.derives_from(class, v));
    let generic = value_class || NOT_RUNTIME.contains(&name.as_str());
    if generic { "genericWrapArray" } else { "wrapRefArray" }.into()
}
