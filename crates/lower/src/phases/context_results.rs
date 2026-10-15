//! Methods whose results are context closures take the closures' parameters as their own, and
//! their calls the closures' arguments (dotty's `ContextFunctionResults`).

use super::casts::function_class_arity;
use super::walk::subtrees;
use crate::symbols::is_effectively_final;
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashSet;
use tasty::{Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// A method of the unit whose context closures became parameter clauses: its symbol, its
/// original parameter clauses, how many closures.
struct Flattened {
    m: SymbolId,
    clauses: usize,
    depth: usize,
}

/// Flattens the methods of unit `root` whose right-hand sides are context closures of their
/// context function result types (dotty's `annotateContextResults`) and their calls; refuses
/// those that may override or be overridden, other references to them, and references to
/// library methods annotated `ContextResultCount`.
pub fn transform(l: &mut Lower, root: TNodeId) -> Result<()> {
    let mut flattened = Vec::new();
    let mut defined = FxHashSet::default();
    for t in subtrees(l.tpd, root) {
        let Tree::DefDef { tpt, rhs, .. } = *l.tpd.tree(t) else { continue };
        let Some(m) = l.tpd.sym(t) else { continue };
        defined.insert(m);
        let Some(rhs) = rhs else { continue };
        if l.u.symbol(m).name == "$anonfun"
            || !is_context_function(l, l.tpd.tpe(tpt))
            || closure_def(l, rhs).is_none()
        {
            continue;
        }
        if !is_alone(l, m) {
            return unsupported(l.tpd.span(t), "overridable methods with context function results");
        }
        let info = l.u.info(m);
        let clauses = method_clauses(l, info);
        let depth = integrate(l, t, m);
        flattened.push(Flattened { m, clauses, depth });
    }
    let mut calls = Vec::new();
    for t in subtrees(l.tpd, root) {
        if let Some(call) = flat_call(l, t, &flattened) {
            calls.push(call);
        }
    }
    let mut handled = FxHashSet::default();
    for (applies, r) in calls {
        handled.insert(r);
        for a in applies {
            let Tree::Apply { fun, args } = *l.tpd.tree(a) else { unreachable!("an apply") };
            let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { unreachable!("a select") };
            l.tpd.get_mut(a).tree = Tree::Apply { fun: qualifier, args };
        }
    }
    for t in subtrees(l.tpd, root) {
        if !matches!(l.tpd.tree(t), Tree::Ident { .. } | Tree::Select { .. }) || handled.contains(&t) {
            continue;
        }
        let Some(s) = l.tpd.sym(t) else { continue };
        let other_unit = !defined.contains(&s) && l.u.symbol(s).tree.is_none() && has_context_result(l, s);
        if flattened.iter().any(|f| f.m == s) || is_annotated(l, s) || other_unit {
            return unsupported(l.tpd.span(t), "methods with context function results");
        }
    }
    Ok(())
}

/// Whether `s` is a method whose final result type is a context function.
fn has_context_result(l: &mut Lower, s: SymbolId) -> bool {
    if l.u.symbol(s).kind != Kind::Def {
        return false;
    }
    let mut t = l.u.info(s);
    while let Type::Poly { result, .. } | Type::Method { result, .. } = *l.u.types().get(t) {
        t = result;
    }
    is_context_function(l, t)
}

/// Whether method `m` neither overrides nor can be overridden: local, or effectively final in a
/// class whose parents are `Object` or `Any` only.
fn is_alone(l: &mut Lower, m: SymbolId) -> bool {
    let Some(owner) = l.u.symbol(m).owner else { return false };
    if l.u.symbol(owner).kind != Kind::Class {
        return true;
    }
    if !is_effectively_final(l.u, owner, m) {
        return false;
    }
    let parents = l.u.parents(owner);
    parents.into_iter().all(|p| {
        l.u.class_symbol(p)
            .is_some_and(|c| matches!(l.u.full_name(c).as_str(), "java.lang.Object" | "scala.Any"))
    })
}

/// How many parameter clauses method type `t` has.
fn method_clauses(l: &Lower, mut t: TypeId) -> usize {
    let mut n = 0;
    loop {
        match l.u.types().get(t) {
            Type::Poly { result, .. } => t = *result,
            Type::Method { result, .. } => (n, t) = (n + 1, *result),
            _ => return n,
        }
    }
}

/// Moves the parameters and bodies of the context closures of method definition `def` of `m`
/// into it; returns how many there were.
fn integrate(l: &mut Lower, def: TNodeId, m: SymbolId) -> usize {
    let Tree::DefDef { name, paramss, mut tpt, rhs: Some(mut rhs) } = *l.tpd.tree(def) else {
        unreachable!("a method with a body")
    };
    let mut clauses = l.tpd.clause_items(paramss).to_vec();
    let mut anons = Vec::new();
    while is_context_function(l, l.tpd.tpe(tpt)) {
        let Some(anon_def) = closure_def(l, rhs) else { break };
        let Tree::DefDef { paramss: ap, tpt: at, rhs: Some(ar), .. } = *l.tpd.tree(anon_def) else {
            unreachable!("a closure's method")
        };
        clauses.extend_from_slice(l.tpd.clause_items(ap));
        anons.push(l.tpd.sym(anon_def).expect("a closure's method symbol"));
        (tpt, rhs) = (at, ar);
    }
    let params: Vec<TNodeId> = clauses.iter().flat_map(|&c| l.tpd.items(c).to_vec()).collect();
    for t in params.into_iter().chain(subtrees(l.tpd, rhs)) {
        let Some(s) = l.tpd.sym(t) else { continue };
        if l.u.symbol(s).owner.is_some_and(|o| anons.contains(&o)) {
            l.u.set_owner(s, m);
        }
    }
    let mut info = l.u.info(*anons.last().expect("a context closure"));
    for &a in anons.iter().rev().skip(1) {
        let outer = l.u.info(a);
        info = with_result(l, outer, info);
    }
    let own = l.u.info(m);
    let info = with_result(l, own, info);
    l.u.set_info(m, info);
    let paramss = l.tpd.clauses(&clauses);
    let node = l.tpd.get_mut(def);
    node.tree = Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) };
    node.tpe = Some(info);
    anons.len()
}

/// Method type `t` with its final result `result`.
fn with_result(l: &mut Lower, t: TypeId, result: TypeId) -> TypeId {
    let t = match l.u.types().get(t).clone() {
        Type::Poly { binder, params, result: r } => {
            Type::Poly { binder, params, result: with_result(l, r, result) }
        }
        Type::Method { binder, params, result: r, implicit, contextual } => {
            Type::Method { binder, params, result: with_result(l, r, result), implicit, contextual }
        }
        _ => return result,
    };
    l.u.intern_type(t)
}

/// The `apply` calls of a call `t` of a flattened method through its context closures,
/// innermost first, and the reference to the method.
fn flat_call(l: &mut Lower, t: TNodeId, flattened: &[Flattened]) -> Option<(Vec<TNodeId>, TNodeId)> {
    for f in flattened {
        let mut applies = Vec::new();
        let mut r = t;
        for _ in 0..f.depth {
            let Tree::Apply { fun, .. } = *l.tpd.tree(r) else { break };
            let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { break };
            if !is_context_apply(l, fun) {
                break;
            }
            applies.push(r);
            r = qualifier;
        }
        if applies.len() == f.depth && references(l, r, f) {
            applies.reverse();
            return Some((applies, method_ref(l, r)));
        }
    }
    None
}

/// Whether `t` applies flattened method `f` to all of its original parameter clauses.
fn references(l: &Lower, mut t: TNodeId, f: &Flattened) -> bool {
    let mut applied = 0;
    loop {
        match *l.tpd.tree(t) {
            Tree::Apply { fun, .. } => (applied, t) = (applied + 1, fun),
            Tree::TypeApply { fun, .. } => t = fun,
            Tree::Ident { .. } | Tree::Select { .. } => {
                return applied == f.clauses && l.tpd.sym(t) == Some(f.m);
            }
            _ => return false,
        }
    }
}

/// The method reference `t` applies.
fn method_ref(l: &Lower, mut t: TNodeId) -> TNodeId {
    while let Tree::Apply { fun, .. } | Tree::TypeApply { fun, .. } = *l.tpd.tree(t) {
        t = fun;
    }
    t
}

/// Whether `t` selects the `apply` of a `ContextFunctionN`.
fn is_context_apply(l: &mut Lower, t: TNodeId) -> bool {
    let Some(s) = l.tpd.sym(t) else { return false };
    let owner = l.u.symbol(s).owner;
    l.u.symbol(s).name == "apply"
        && owner.and_then(|c| function_class_arity(l, c)).is_some_and(|(_, context)| context)
}

/// Whether library method `s` is annotated `ContextResultCount`.
fn is_annotated(l: &Lower, s: SymbolId) -> bool {
    l.u.symbol(s).tree.is_some() && l.u.is_annotated(s, "ContextResultCount")
}

/// Whether `t` is a `ContextFunctionN` type.
fn is_context_function(l: &mut Lower, t: TypeId) -> bool {
    let t = l.u.dealias(t);
    let Type::Applied(tycon, _) = *l.u.types().get(t) else { return false };
    let class = l.u.class_symbol(tycon);
    class.and_then(|c| function_class_arity(l, c)).is_some_and(|(_, context)| context)
}

/// The method definition `t` is a closure of, in blocks without statements (dotty's
/// `closureDef`).
fn closure_def(l: &Lower, t: TNodeId) -> Option<TNodeId> {
    let Tree::Block { stats, expr } = *l.tpd.tree(t) else { return None };
    match l.tpd.items(stats) {
        [] => closure_def(l, expr),
        &[def] => {
            let is_closure =
                matches!(*l.tpd.tree(expr), Tree::Closure { meth, .. } if l.tpd.sym(meth) == l.tpd.sym(def));
            is_closure.then_some(def)
        }
        _ => None,
    }
}
