// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/ContextFunctionResults.scala
//! Methods whose results are context closures take the closures' parameters as their own, and
//! their calls the closures' arguments (dotty's `ContextFunctionResults`).

use super::casts::function_class_arity;
use super::walk::subtrees;
use crate::{Lower, Result, unsupported};
use rustc_hash::{FxHashMap, FxHashSet};
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
/// those overriding or overridden in the unit by methods flattened otherwise, those overriding
/// members of other units, other references to them, and references to library methods
/// annotated `ContextResultCount`.
pub fn transform(l: &mut Lower, root: TNodeId) -> Result<()> {
    let mut defs = Vec::new();
    let mut counts = FxHashMap::default();
    for t in subtrees(l.tpd, root) {
        let Tree::DefDef { tpt, rhs, .. } = *l.tpd.tree(t) else { continue };
        let Some(m) = l.tpd.sym(t) else { continue };
        defs.push(m);
        let count = rhs.map_or(0, |rhs| context_result_count(l, tpt, rhs));
        if count > 0 && l.u.symbol(m).name != "$anonfun" {
            counts.insert(m, (t, count));
        }
    }
    let defined: FxHashSet<SymbolId> = defs.iter().copied().collect();
    let mut flattened = Vec::new();
    for t in subtrees(l.tpd, root) {
        let Some(m) = l.tpd.sym(t).filter(|m| counts.get(m).is_some_and(|&(d, _)| d == t)) else { continue };
        if !overrides_alike(l, m, &defs, &counts) {
            return unsupported(
                l.tpd.span(t),
                "overriding methods with context function results flattened otherwise",
            );
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

/// How many context closures the right-hand side `rhs` of a method of result type tree `tpt`
/// nests (dotty's `contextResultCount`).
fn context_result_count(l: &mut Lower, mut tpt: TNodeId, mut rhs: TNodeId) -> usize {
    let mut n = 0;
    while is_context_function(l, l.tpd.tpe(tpt)) {
        let Some(anon_def) = closure_def(l, rhs) else { break };
        let Tree::DefDef { tpt: at, rhs: Some(ar), .. } = *l.tpd.tree(anon_def) else { break };
        (n, tpt, rhs) = (n + 1, at, ar);
    }
    n
}

/// Whether every member of the name of method `m` that `m` may override, or that may override
/// `m`, is a method of the unit flattened by as many closures: those of the base classes of its
/// class and of the unit's classes deriving from it, and of `Object` and `Any`.
fn overrides_alike(
    l: &mut Lower,
    m: SymbolId,
    defs: &[SymbolId],
    counts: &FxHashMap<SymbolId, (TNodeId, usize)>,
) -> bool {
    let Some(owner) = l.u.symbol(m).owner.filter(|&o| l.u.symbol(o).kind == Kind::Class) else {
        return true;
    };
    let count = counts.get(&m).map(|&(_, c)| c);
    let name = l.u.symbol(m).name.clone();
    let mut classes = vec![owner];
    for &d in defs {
        let o = l.u.symbol(d).owner.filter(|&o| l.u.symbol(o).kind == Kind::Class);
        if let Some(o) = o.filter(|&o| !classes.contains(&o) && l.u.derives_from(o, owner)) {
            classes.push(o);
        }
    }
    let mut related: Vec<SymbolId> = Vec::new();
    for class in classes {
        for base in l.u.linearization(class) {
            related.extend(l.u.decl(base, &name).unwrap_or_default());
        }
    }
    let roots = [("java.lang", "Object"), ("scala", "Any")];
    related.into_iter().all(|d| d == m || counts.get(&d).map(|&(_, c)| c) == count)
        && roots.into_iter().all(|(p, c)| !root_declares(l, p, c, &name))
}

/// Whether class `class` of package `package` declares a member `name`, or cannot be found.
fn root_declares(l: &mut Lower, package: &str, class: &str, name: &str) -> bool {
    let Some(pkg) = l.u.package(package) else { return true };
    let found = l.u.decl(pkg, class).unwrap_or_default();
    let classes: Vec<SymbolId> = found.into_iter().filter(|&c| l.u.symbol(c).kind == Kind::Class).collect();
    classes.is_empty() || classes.into_iter().any(|c| l.u.decl(c, name).map_or(true, |d| !d.is_empty()))
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
/// into it; returns how many there were (dotty's `contextResultCount`, `integrateContextResults`).
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
    let last = l.u.info(*anons.last().expect("a context closure"));
    let mut info = synthetic_names(l, last);
    for &a in anons.iter().rev().skip(1) {
        let outer = l.u.info(a);
        let outer = synthetic_names(l, outer);
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

/// Method type `t` with its parameters named `x$0`, `x$1`, … (a context function's method type).
fn synthetic_names(l: &mut Lower, t: TypeId) -> TypeId {
    let Type::Method { binder, params, result, implicit, contextual } = l.u.types().get(t).clone() else {
        return t;
    };
    let params = params.into_iter().enumerate().map(|(i, (_, p))| (format!("x${i}"), p)).collect();
    l.u.intern_type(Type::Method { binder, params, result, implicit, contextual })
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
    l.u.symbol(s).tree.is_some() && l.u.is_annotated(s, "scala.annotation.internal.ContextResultCount")
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
