// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/ArrayConstructors.scala
//! dotty's `ArrayConstructors` for `Array.ofDim`: `Array.ofDim[T](n)` of a class `T` other than
//! a value class is `new Array[T](n)`, after the prefix of a selected `ofDim`.

use super::walk::subtrees;
use crate::Lower;
use tasty::{Erased, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// Rewrites the calls of `Array.ofDim` of one dimension in class definition `td`.
pub fn transform(l: &mut Lower, td: TNodeId) {
    for t in subtrees(l.tpd, td) {
        if let Some(new) = expanded(l, t) {
            let span = l.tpd.span(t);
            l.copy_node(t, new);
            l.tpd.get_mut(t).span = span;
        }
    }
}

/// `new Array[T](n)` for `t`, `Array.ofDim[T](n)(tag)`.
fn expanded(l: &mut Lower, t: TNodeId) -> Option<TNodeId> {
    let Tree::Apply { fun: dims_call, .. } = *l.tpd.tree(t) else { return None };
    let Tree::Apply { fun: targs_call, args: dims } = *l.tpd.tree(dims_call) else { return None };
    let Tree::TypeApply { fun, args: targs } = *l.tpd.tree(targs_call) else { return None };
    let m = l.tpd.sym(fun)?;
    let owner = l.u.symbol(m).owner?;
    if l.u.symbol(m).name != "ofDim" || l.u.full_name(owner) != "scala.Array$" {
        return None;
    }
    let (&[n], &[targ]) = (l.tpd.items(dims), l.tpd.items(targs)) else { return None };
    let elem = l.tpd.tpe(targ);
    if !is_class(l, elem) || l.u.value_class(elem).is_some() {
        return None;
    }
    let elem = match l.u.erase(elem) {
        Erased::Unit => l.u.class_ref("scala.runtime.BoxedUnit"),
        _ => elem,
    };
    let span = l.tpd.span(t);
    let new = new_array(l, elem, n, span)?;
    match *l.tpd.tree(fun) {
        Tree::Select { qualifier, .. } => Some(l.block(&[qualifier], new, span)),
        _ => Some(new),
    }
}

/// Whether `t` is a class type, not a type parameter or abstract type (dotty's `isGeneric`).
fn is_class(l: &mut Lower, t: TypeId) -> bool {
    let t = l.u.dealias(t);
    let t = match *l.u.types().get(t) {
        Type::Applied(tycon, _) => tycon,
        _ => t,
    };
    l.u.referenced(t).is_some_and(|s| l.u.is_class(s))
}

/// `new Array[elem](n)` at `span`.
fn new_array(l: &mut Lower, elem: TypeId, n: TNodeId, span: syntax::ast::Span) -> Option<TNodeId> {
    let array = l.u.class_ref("scala.Array");
    let class = l.u.class_symbol(array)?;
    let ctor = *l.u.decl(class, "<init>").ok()?.first()?;
    let tpe = l.u.intern_type(Type::Applied(array, vec![elem]));
    let tpt = l.type_tree(tpe, span);
    let new = l.tpd.node(Tree::New { tpt }, span, Some(tpe), None);
    let sel = l.select(new, ctor, span);
    Some(l.apply(sel, &[n], tpe, span))
}
