// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
//   compiler/src/dotty/tools/dotc/transform/ArrayConstructors.scala
//! dotty's `ArrayConstructors` for `Array.ofDim`: `Array.ofDim[T](n)` of a non-generic `T` other
//! than a value class is `new Array[T](n)`, of several dimensions
//! `scala.runtime.Arrays.newArray(classOf[T], classOf[R], Array(n, ..))`, after the prefix of a
//! selected `ofDim`.

use super::walk::subtrees;
use crate::Lower;
use crate::definitions::applied;
use syntax::ast::Span;
use tasty::{Erased, Type, TypeId, nme};
use typer::tpd::{TNodeId, Tree};

/// Rewrites the calls of `Array.ofDim` in class definition `td`.
pub fn transform(l: &mut Lower, td: TNodeId) {
    for t in subtrees(l.tpd, td) {
        if let Some(new) = expanded(l, t) {
            let span = l.tpd.span(t);
            l.copy_node(t, new);
            l.tpd.get_mut(t).span = span;
        }
    }
}

/// The array `t`, `Array.ofDim[T](n, ..)(tag)`, makes.
fn expanded(l: &mut Lower, t: TNodeId) -> Option<TNodeId> {
    let Tree::Apply { fun: dims_call, .. } = *l.tpd.tree(t) else { return None };
    let Tree::Apply { fun: targs_call, args: dims } = *l.tpd.tree(dims_call) else { return None };
    let Tree::TypeApply { fun, args: targs } = *l.tpd.tree(targs_call) else { return None };
    let m = l.tpd.sym(fun)?;
    let owner = l.u.symbol(m).owner?;
    if l.u.symbol(m).name != nme::OF_DIM || !l.is_known(owner, crate::definitions::Known::ArrayModuleClass) {
        return None;
    }
    let (dims, &[targ]) = (l.tpd.items(dims).to_vec(), l.tpd.items(targs)) else { return None };
    let elem = l.tpd.tpe(targ);
    if dims.is_empty() || is_generic(l, elem) || l.u.value_class(elem).is_some() {
        return None;
    }
    let elem = match l.u.erase(elem) {
        Erased::Unit => l.u.class_ref(tasty::defn::BOXED_UNIT),
        _ => elem,
    };
    let span = l.tpd.span(t);
    let new = match dims[..] {
        [n] => new_array(l, elem, n, span)?,
        _ => new_arrays(l, elem, l.tpd.tpe(t), &dims, span)?,
    };
    match *l.tpd.tree(fun) {
        Tree::Select { qualifier, .. } => Some(l.block(&[qualifier], new, span)),
        _ => Some(new),
    }
}

/// Whether `t` may be other than a class type (dotty's `TypeErasure.isGeneric`).
fn is_generic(l: &mut Lower, t: TypeId) -> bool {
    let t = l.u.dealias(t);
    match *l.u.ty(t) {
        Type::Applied(u, _) | Type::Annotated(u, _) | Type::Refined(u, _, _) => is_generic(l, u),
        Type::And(a, b) | Type::Or(a, b, _) => is_generic(l, a) || is_generic(l, b),
        Type::Param(..) | Type::Local(..) => true,
        _ => match l.u.referenced(t) {
            Some(s) if l.u.is_class(s) => false,
            Some(s) => l.u.opaque_alias(s).is_none_or(|alias| is_generic(l, alias)),
            None => false,
        },
    }
}

/// `new Array[elem](n)` at `span`.
fn new_array(l: &mut Lower, elem: TypeId, n: TNodeId, span: Span) -> Option<TNodeId> {
    let array = l.u.class_ref(tasty::defn::ARRAY);
    let class = l.u.class_symbol(array)?;
    let ctor = *l.u.decl(class, nme::CONSTRUCTOR).ok()?.first()?;
    let tpe = l.u.intern_applied(array, &[elem]);
    let tpt = l.type_tree(tpe, span);
    let new = l.tpd.node(Tree::New { tpt }, span, Some(tpe), None);
    let sel = l.select(new, ctor, span);
    Some(l.apply(sel, &[n], tpe, span))
}

/// `scala.runtime.Arrays.newArray[result](classOf[elem], classOf[result], Array(dims))` at
/// `span` (dotty's `tpd.newArray`).
fn new_arrays(l: &mut Lower, elem: TypeId, result: TypeId, dims: &[TNodeId], span: Span) -> Option<TNodeId> {
    let runtime = l.u.package("scala.runtime")?;
    let module = l.u.decl(runtime, "Arrays").ok()?.into_iter().find(|&m| l.u.is_module(m))?;
    let class = l.u.module_class(module)?;
    let method = *l.u.decl(class, nme::NEW_ARRAY).ok()?.first()?;
    let qual = l.ident(module, span);
    let fun = l.select(qual, method, span);
    let targ = l.type_tree(result, span);
    let info = l.u.member_info(l.tpd.tpe(qual), method);
    let fun = l.build().type_apply(fun, &[targ], info, span);
    let class_of = l.u.class_ref(tasty::defn::CLASS);
    let class_of = l.u.class_symbol(class_of)?;
    let literal = |l: &mut Lower, tpe| {
        let class = applied(l.u, class_of, vec![tpe]);
        l.tpd.node(Tree::ClassLiteral { tpe }, span, Some(class), None)
    };
    let (elem_class, result_class) = (literal(l, elem), literal(l, result));
    let int = l.u.class_ref(tasty::defn::INT);
    let elemtpt = l.type_tree(int, span);
    let array = l.u.class_ref(tasty::defn::ARRAY);
    let ints = l.u.intern_applied(array, &[int]);
    let elems = l.tpd.list(dims);
    let dims = l.tpd.node(Tree::SeqLiteral { elems, elemtpt }, span, Some(ints), None);
    Some(l.apply(fun, &[elem_class, result_class, dims], result, span))
}
