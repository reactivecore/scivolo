// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/ElimRepeated.scala
//! Repeated arguments (dotty's `ElimRepeated` and `SeqLiterals`): the `SeqLiteral` passed to a
//! Java varargs parameter becomes an array, one passed to a Scala repeated parameter an array
//! wrapped by `ScalaRunTime.wrapXArray`.

use super::array_wraps::{array_of, repeated_array, seq_value, wrap_array, wrap_seq};
use super::walk::try_walk_class;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::{Erased, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree, Walk};

/// Rewrites the repeated arguments of the calls in class definition `td`.
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    try_walk_class(l, td, &mut |l, t, _| {
        if let Tree::SeqLiteral { elems, elemtpt } = *l.tpd.tree(t) {
            seq_value(l, t, elems, elemtpt)?;
            return Ok(Walk::Children);
        }
        if let Tree::ValDef { name, tpt, rhs: Some(rhs) } = *l.tpd.tree(t)
            && let Some(wrapped) = repeated_array(l, rhs)?
        {
            l.tpd.get_mut(t).tree = Tree::ValDef { name, tpt, rhs: Some(wrapped) };
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
    if !l.u.is_class_named(ascribed, tasty::defn::REPEATED) {
        return Ok(a);
    }
    let span = l.tpd.span(a);
    let Some((method, clause)) = callee(l, app) else {
        return unsupported(span, "repeated arguments of a call without a method symbol");
    };
    let Tree::SeqLiteral { elemtpt, .. } = *l.tpd.tree(seq) else {
        let tpe = l.u.widen(l.tpd.tpe(seq));
        let Some((_, elems)) = l.u.applied(ascribed) else { return Ok(seq) };
        let elem = elems[0];
        if java_varargs(l, method, clause, i).is_some() {
            return to_array(l, seq, elem, span);
        }
        if l.u.is_class_named(tpe, tasty::defn::ARRAY) {
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
    wrap_seq(l, seq, elem, ascribed, span)
}

/// Sequence `seq` of elements of type `elem` passed to Java varargs: an array as it is, another
/// `scala.runtime.Arrays.seqToArray(seq, classOf[elem])`, ascribed `Array[elem]` (dotty's
/// `adaptToArray`).
fn to_array(l: &mut Lower, seq: TNodeId, elem: TypeId, span: Span) -> Result<TNodeId> {
    let array = array_of(l, elem);
    let tpe = l.u.widen(l.tpd.tpe(seq));
    let adapted = if l.u.is_class_named(tpe, tasty::defn::ARRAY) {
        seq
    } else {
        let runtime = l.u.package("scala.runtime").expect("package scala.runtime");
        let module = l.u.decls_named(runtime, "Arrays");
        let module = module.into_iter().find(|&m| l.u.is_module(m));
        let class = module.and_then(|m| l.u.module_class(m));
        let method = class.and_then(|c| l.u.decls_named(c, "seqToArray").first().copied());
        let (Some(module), Some(method)) = (module, method) else {
            return unsupported(span, "scala.runtime.Arrays.seqToArray missing from the library");
        };
        let qual = l.ident(module, Span::NONE);
        let fun = l.select(qual, method, Span::NONE);
        let targ = l.type_tree(elem, Span::NONE);
        let info = l.u.member_info(l.tpd.tpe(qual), method);
        let fun = l.build().type_apply(fun, &[targ], info, Span::NONE);
        let class_of = l.u.class_ref(tasty::defn::CLASS);
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
        match *l.u.ty(t) {
            Type::Poly { result, .. } => t = result,
            Type::Method { result, .. } if clauses < clause => (t, clauses) = (result, clauses + 1),
            Type::Method { params, .. } => {
                let p = l.u.params_of(params).get(i)?.1;
                return java_array(l, p);
            }
            _ => return None,
        }
    }
}

/// The array of parameter type `p` if it is Java varargs, an array marked repeated.
pub fn java_array(l: &mut Lower, p: TypeId) -> Option<TypeId> {
    let (inner, annot) = l.u.annotated(p)?;
    let array = l.u.applied(inner).map(|(c, _)| c).is_some_and(|c| l.u.is_class_named(c, tasty::defn::ARRAY));
    let repeated = l.u.is_class_named(annot, "scala.annotation.internal.Repeated");
    (array && repeated).then_some(inner)
}
