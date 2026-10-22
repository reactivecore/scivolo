// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/EtaReduce.scala
//! dotty's `EtaReduce`: `{ def $anonfun(x1, ..., xN) = f.apply(x1, ..., xN); closure($anonfun) }`
//! becomes `f` if `f` is a pure path, not a getter call, of a function type conforming to the
//! closure's, unless erasure gives the closure a specialized interface: some `xi` primitive, or a
//! `Unit` result of a function literal.

use super::by_name::is_by_name_param;
use super::casts::conforms;
use super::first_transform::is_pure;
use super::walk::subtrees;
use crate::Lower;
use crate::symbols::has;
use tasty::tags::{LAZY, LOCAL, PRIVATE};
use tasty::{Erased, Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// Reduces the eta expansions in class definition `td`.
pub fn transform(l: &mut Lower, td: TNodeId) {
    for t in subtrees(l.tpd, td) {
        if let Some(f) = reduced(l, t) {
            l.copy_node(t, f);
        }
    }
}

/// The function `t` eta-expands, if it is a block of a lambda's definition and its closure.
fn reduced(l: &mut Lower, t: TNodeId) -> Option<TNodeId> {
    let Tree::Block { stats, expr } = *l.tpd.tree(t) else { return None };
    let &[def] = l.tpd.items(stats) else { return None };
    let (Tree::DefDef { paramss, rhs: Some(rhs), .. }, Some(meth)) =
        (l.tpd.tree(def).clone(), l.tpd.sym(def))
    else {
        return None;
    };
    let Tree::Closure { meth: closure_meth, tpt, .. } = *l.tpd.tree(expr) else { return None };
    if l.tpd.sym(closure_meth) != Some(meth) || l.u.symbol(meth).name != "$anonfun" {
        return None;
    }
    let meth_info = l.u.info(meth);
    let result = l.u.result_type(meth_info);
    if tpt.is_none() && l.u.erase(result) == Erased::Unit {
        return None;
    }
    let Tree::Apply { fun, args } = *l.tpd.tree(rhs) else { return None };
    let Tree::Select { qualifier: f, .. } = *l.tpd.tree(fun) else { return None };
    if l.tpd.sym(fun).is_none_or(|m| l.u.symbol(m).name != "apply") {
        return None;
    }
    let params: Vec<Option<SymbolId>> =
        l.tpd.clause_items(paramss).iter().flat_map(|&c| l.tpd.items(c)).map(|&p| l.tpd.sym(p)).collect();
    let args: Vec<Option<SymbolId>> = l
        .tpd
        .items(args)
        .iter()
        .map(|&a| match l.tpd.tree(a) {
            Tree::Ident { .. } => l.tpd.sym(a),
            _ => None,
        })
        .collect();
    let own_param = l.tpd.sym(f).is_some_and(|s| l.u.symbol(s).owner == Some(meth));
    let getter = l.tpd.sym(f).is_some_and(|s| has_getter(l, s));
    if params != args || params.contains(&None) || own_param || getter || !is_pure(l, f) {
        return None;
    }
    let boxed = params.iter().flatten().any(|&p| {
        let info = l.u.info(p);
        matches!(l.u.erase(info), Erased::Prim(_) | Erased::Unit)
    });
    if boxed {
        return None;
    }
    let from = function_erasure(l, f)?;
    let to = l.u.erase(l.tpd.tpe(t));
    conforms(l, &from, &to).then_some(f)
}

/// Whether value `s` of a class is read by a getter call (dotty's `Getters`): not a parameter, an
/// object, nor a private local non-lazy field of a class.
fn has_getter(l: &Lower, s: SymbolId) -> bool {
    let Some(owner) = l.u.symbol(s).owner else { return false };
    if l.u.symbol(s).kind != Kind::Val || !l.u.is_class(owner) || l.u.is_module(s) {
        return false;
    }
    let field = has(l.u, s, PRIVATE) && has(l.u, s, LOCAL) && !has(l.u, s, LAZY) && !l.u.is_trait(owner);
    !field
}

/// The erased `FunctionN` class of `f`'s type, a by-name parameter's `Function0`; `None` if not a
/// function type.
fn function_erasure(l: &mut Lower, f: TNodeId) -> Option<Erased> {
    let s = l.tpd.sym(f)?;
    if is_by_name_param(l, s) {
        return Some(Erased::Class("scala.Function0".into()));
    }
    let info = l.u.info(s);
    let tpe = l.u.widen(info);
    let class = l.u.class_symbol(tpe)?;
    super::function_classes::function_class_arity(l, class)?;
    Some(l.u.erase(tpe))
}
