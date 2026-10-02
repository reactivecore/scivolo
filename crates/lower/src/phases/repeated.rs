//! Repeated arguments (dotty's `ElimRepeated` and `SeqLiterals`): the `SeqLiteral` passed to a
//! Java varargs parameter becomes an array, one passed to a Scala repeated parameter an array
//! wrapped by `ScalaRunTime.wrapXArray`.

use super::walk::children;
use crate::{Lower, Result, unsupported};
use tasty::{Erased, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// The classes whose arrays `ScalaRunTime` wraps by their own method.
const PRIMITIVES: [&str; 9] = ["Boolean", "Byte", "Char", "Short", "Int", "Long", "Float", "Double", "Unit"];

/// The classes without instances of their own at run time, wrapped generically.
const NOT_RUNTIME: [&str; 6] =
    ["scala.Any", "scala.AnyVal", "scala.Matchable", "scala.Nothing", "scala.Null", "scala.Singleton"];

/// Rewrites the repeated arguments of the calls in class definition `td`.
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let mut todo = vec![td];
    while let Some(t) = todo.pop() {
        if let Tree::Apply { fun, args } = *l.tpd.tree(t) {
            let old = l.tpd.items(args).to_vec();
            let mut new = Vec::new();
            for (i, &a) in old.iter().enumerate() {
                new.push(repeated_arg(l, t, i, a)?);
            }
            if new != old {
                let args = l.tpd.list(&new);
                l.tpd.get_mut(t).tree = Tree::Apply { fun, args };
            }
        }
        let nested = matches!(l.tpd.tree(t), Tree::TypeDef { .. }) && t != td;
        if !nested {
            todo.extend(children(l.tpd, t));
        }
    }
    Ok(())
}

/// Argument `a`, the `i`th of call `app`, rewritten if it is `seq: <repeated>[T]`.
fn repeated_arg(l: &mut Lower, app: TNodeId, i: usize, a: TNodeId) -> Result<TNodeId> {
    let Tree::Typed { expr: seq, tpt } = *l.tpd.tree(a) else { return Ok(a) };
    let ascribed = l.tpd.tpe(tpt);
    if !is_class(l, ascribed, "scala.<repeated>") {
        return Ok(a);
    }
    let span = l.tpd.span(a);
    let Tree::SeqLiteral { elemtpt, .. } = *l.tpd.tree(seq) else {
        return unsupported(span, "sequences passed as repeated arguments");
    };
    let Some((method, clause)) = callee(l, app) else {
        return unsupported(span, "repeated arguments of a call without a method symbol");
    };
    let elem = l.tpd.tpe(elemtpt);
    let array = l.u.class_ref("scala.Array");
    let array = l.u.intern_type(Type::Applied(array, vec![elem]));
    l.tpd.get_mut(seq).tpe = Some(array);
    if let Some(param) = java_varargs(l, method, clause, i) {
        let (ours, theirs) = (l.u.erase(array), l.u.erase(param));
        if ours != theirs && matches!(&ours, Erased::Array(e) if matches!(**e, Erased::Prim(_))) {
            return unsupported(span, "primitive arrays passed to Java varargs of references");
        }
        return Ok(seq);
    }
    let wrap = wrapper(l, elem);
    let module = l.defs.scala_run_time;
    let class = l.u.module_class(module).expect("ScalaRunTime's class");
    let Some(&wrap) = l.u.decl(class, wrap).unwrap_or_default().first() else {
        return unsupported(span, format!("ScalaRunTime.{wrap} missing from the library"));
    };
    let qual = l.ident(module, span);
    let fun = l.select(qual, wrap, span);
    Ok(l.apply(fun, &[seq], ascribed, span))
}

/// The method call `app` calls and the index of the parameter clause its arguments are for.
fn callee(l: &Lower, app: TNodeId) -> Option<(SymbolId, usize)> {
    let Tree::Apply { mut fun, .. } = *l.tpd.tree(app) else { return None };
    let mut clause = 0;
    loop {
        match *l.tpd.tree(fun) {
            Tree::Apply { fun: f, .. } => (fun, clause) = (f, clause + 1),
            Tree::TypeApply { fun: f, .. } => fun = f,
            _ => return Some((l.tpd.sym(fun)?, clause)),
        }
    }
}

/// The type of the `i`th parameter of clause `clause` of `method` if it is Java varargs: an
/// array marked repeated.
fn java_varargs(l: &mut Lower, method: SymbolId, clause: usize, i: usize) -> Option<TypeId> {
    let mut t = l.u.info(method);
    let mut clauses = 0;
    loop {
        match l.u.types().get(t).clone() {
            Type::Poly { result, .. } => t = result,
            Type::Method { result, .. } if clauses < clause => (t, clauses) = (result, clauses + 1),
            Type::Method { params, .. } => {
                let &(_, p) = params.get(i)?;
                let Type::Annotated(inner, annot) = l.u.types().get(p).clone() else { return None };
                let array = matches!(l.u.types().get(inner).clone(),
                    Type::Applied(c, _) if is_class(l, c, "scala.Array"));
                let repeated = is_class(l, annot, "scala.annotation.internal.Repeated");
                return (array && repeated).then_some(inner);
            }
            _ => return None,
        }
    }
}

/// The `ScalaRunTime` method wrapping an array of `elem` (dotty's `wrapArrayMethodName`).
fn wrapper(l: &mut Lower, elem: TypeId) -> &'static str {
    let Some(class) = l.u.class_symbol(elem).filter(|&c| l.u.symbol(c).kind == Kind::Class) else {
        return "genericWrapArray";
    };
    let name = l.u.full_name(class);
    if let Some(i) = name.strip_prefix("scala.").and_then(|p| PRIMITIVES.iter().position(|&q| q == p)) {
        return WRAPPERS[i];
    }
    let value_class = l.u.builtins().any_val.is_some_and(|v| l.u.derives_from(class, v));
    if value_class || NOT_RUNTIME.contains(&name.as_str()) { "genericWrapArray" } else { "wrapRefArray" }
}

/// The `ScalaRunTime` methods wrapping arrays of `PRIMITIVES`, in their order.
const WRAPPERS: [&str; 9] = [
    "wrapBooleanArray",
    "wrapByteArray",
    "wrapCharArray",
    "wrapShortArray",
    "wrapIntArray",
    "wrapLongArray",
    "wrapFloatArray",
    "wrapDoubleArray",
    "wrapUnitArray",
];

fn is_class(l: &mut Lower, t: TypeId, full: &str) -> bool {
    l.u.class_symbol(t).is_some_and(|c| l.u.full_name(c) == full)
}
