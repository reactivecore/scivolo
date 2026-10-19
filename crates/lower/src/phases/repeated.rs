//! Repeated arguments (dotty's `ElimRepeated` and `SeqLiterals`): the `SeqLiteral` passed to a
//! Java varargs parameter becomes an array, one passed to a Scala repeated parameter an array
//! wrapped by `ScalaRunTime.wrapXArray`.

use super::walk::try_walk_class;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::{Erased, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree, Walk};

/// The classes whose arrays `ScalaRunTime` wraps by their own method.
const PRIMITIVES: [&str; 9] = ["Boolean", "Byte", "Char", "Short", "Int", "Long", "Float", "Double", "Unit"];

/// The classes without instances of their own at run time, wrapped generically.
const NOT_RUNTIME: [&str; 6] =
    ["scala.Any", "scala.AnyVal", "scala.Matchable", "scala.Nothing", "scala.Null", "scala.Singleton"];

/// Rewrites the repeated arguments of the calls in class definition `td`.
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    try_walk_class(l, td, &mut |l, t, _| {
        if let Tree::SeqLiteral { elems, elemtpt } = *l.tpd.tree(t) {
            seq_value(l, t, elems, elemtpt)?;
            return Ok(Walk::Children);
        }
        let Tree::Apply { fun, args } = *l.tpd.tree(t) else { return Ok(Walk::Children) };
        let old = l.tpd.items(args).to_vec();
        let mut new = Vec::new();
        for (i, &a) in old.iter().enumerate() {
            new.push(repeated_arg(l, t, i, a)?);
        }
        if new != old {
            let args = l.tpd.list(&new);
            l.tpd.get_mut(t).tree = Tree::Apply { fun, args };
        }
        Ok(Walk::Children)
    })
}

/// Argument `a`, the `i`th of call `app`, rewritten if it is `seq: <repeated>[T]`.
fn repeated_arg(l: &mut Lower, app: TNodeId, i: usize, a: TNodeId) -> Result<TNodeId> {
    let Tree::Typed { expr: seq, tpt } = *l.tpd.tree(a) else { return Ok(a) };
    let ascribed = l.tpd.tpe(tpt);
    if !is_class(l, ascribed, "scala.<repeated>") {
        return Ok(a);
    }
    let span = l.tpd.span(a);
    let Some((method, clause)) = callee(l, app) else {
        return unsupported(span, "repeated arguments of a call without a method symbol");
    };
    let Tree::SeqLiteral { elemtpt, .. } = *l.tpd.tree(seq) else {
        let tpe = l.u.widen(l.tpd.tpe(seq));
        let Type::Applied(_, ref elems) = *l.u.types().get(ascribed) else { return Ok(seq) };
        let elem = elems[0];
        if java_varargs(l, method, clause, i).is_some() {
            return to_array(l, seq, elem, span);
        }
        if is_class(l, tpe, "scala.Array") {
            let wrapped = wrap_array(l, seq, elem, ascribed, Span::NONE)?;
            return Ok(l.positioned(wrapped));
        }
        return Ok(seq);
    };
    let elem = l.tpd.tpe(elemtpt);
    let array = array_of(l, elem);
    l.tpd.get_mut(seq).tpe = Some(array);
    if let Some(param) = java_varargs(l, method, clause, i) {
        let (ours, theirs) = (l.u.erase(array), l.u.erase(param));
        if ours != theirs && matches!(&ours, Erased::Array(e) if matches!(**e, Erased::Prim(_))) {
            return unsupported(span, "primitive arrays passed to Java varargs of references");
        }
        return Ok(seq);
    }
    wrap_array(l, seq, elem, ascribed, span)
}

/// Sequence literal `t` of `elems` outside a repeated argument, of a Scala sequence type, made
/// its elements' array wrapped (dotty's `SeqLiterals`).
fn seq_value(l: &mut Lower, t: TNodeId, elems: typer::tpd::List, elemtpt: TNodeId) -> Result<()> {
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
fn wrap_array(l: &mut Lower, array: TNodeId, elem: TypeId, tpe: TypeId, span: Span) -> Result<TNodeId> {
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

/// Sequence `seq` of elements of type `elem` passed to Java varargs: an array as it is, another
/// `scala.runtime.Arrays.seqToArray(seq, classOf[elem])`, ascribed `Array[elem]` (dotty's
/// `adaptToArray`).
fn to_array(l: &mut Lower, seq: TNodeId, elem: TypeId, span: Span) -> Result<TNodeId> {
    let array = array_of(l, elem);
    let tpe = l.u.widen(l.tpd.tpe(seq));
    let adapted = if is_class(l, tpe, "scala.Array") {
        seq
    } else {
        let runtime = l.u.package("scala.runtime").expect("package scala.runtime");
        let module = l.u.decl(runtime, "Arrays").unwrap_or_default();
        let module = module.into_iter().find(|&m| l.u.is_module(m));
        let class = module.and_then(|m| l.u.module_class(m));
        let method = class.and_then(|c| l.u.decl(c, "seqToArray").unwrap_or_default().first().copied());
        let (Some(module), Some(method)) = (module, method) else {
            return unsupported(span, "scala.runtime.Arrays.seqToArray missing from the library");
        };
        let qual = l.ident(module, Span::NONE);
        let fun = l.select(qual, method, Span::NONE);
        let targ = l.type_tree(elem, Span::NONE);
        let info = l.u.member_info(l.tpd.tpe(qual), method);
        let fun = l.build().type_apply(fun, &[targ], info, Span::NONE);
        let class_of = l.u.class_ref("java.lang.Class");
        let class_of = l.u.class_symbol(class_of).expect("`java.lang.Class`");
        let class_of = crate::definitions::applied(l.u, class_of, vec![elem]);
        let literal = l.tpd.node(Tree::ClassLiteral { tpe: elem }, Span::NONE, Some(class_of), None);
        let call = l.apply(fun, &[seq, literal], array, Span::NONE);
        l.positioned(call)
    };
    let tpt = l.type_tree(array, Span::NONE);
    let typed = l.tpd.node(Tree::Typed { expr: adapted, tpt }, Span::NONE, Some(array), None);
    Ok(l.positioned(typed))
}

fn array_of(l: &mut Lower, elem: TypeId) -> TypeId {
    let array = l.u.class_ref("scala.Array");
    l.u.intern_type(Type::Applied(array, vec![elem]))
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
fn wrapper(l: &mut Lower, elem: TypeId) -> String {
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

fn is_class(l: &mut Lower, t: TypeId, full: &str) -> bool {
    l.u.class_symbol(t).is_some_and(|c| l.u.full_name(c) == full)
}
