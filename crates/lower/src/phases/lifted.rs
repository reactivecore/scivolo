// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/LambdaLift.scala
//! The methods lifted from lambdas and local methods: named `<owner>$$anonfun$N` and `f$N` from
//! the outermost function in, taking proxies `x$N` of the captured values first; static unless
//! they need an instance. Calls of local methods pass the captured values.

use super::closures::{Plan, plan, rewrite};
use super::lambda_lift::Function;
use super::lift_names::prefix;
use super::narrowed::home;
use super::walk::children;
use crate::definitions::{symbol, type_ref};
use crate::{Lower, Result};
use rustc_hash::FxHashMap;
use tasty::tags::{PRIVATE, STATIC};
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// A lifted method: its symbol and the proxies of the captured values.
pub struct Method {
    pub sym: SymbolId,
    pub proxies: Vec<SymbolId>,
}

/// The methods of `functions` of `class`, named as the functions until `lift`.
pub fn methods(l: &mut Lower, class: SymbolId, functions: &[Function]) -> Result<Vec<Method>> {
    let mut methods = Vec::new();
    for f in functions {
        let (params, result) = signature(l, f)?;
        let name = l.u.symbol(f.sym).name.clone();
        let params: Vec<(String, TypeId)> =
            params.iter().map(|&(p, t)| (l.u.symbol(p).name.clone(), t)).collect();
        let method = define(l, home(l, class, f), &name, &f.captured, &params, result, f.instance());
        if l.local_lazy_accessors.contains(&f.sym) {
            l.local_lazy_accessors.insert(method.sym);
        }
        if let Some(owner) = l.u.symbol(f.sym).owner {
            l.lifted_from.insert(method.sym, owner);
        }
        l.lifted_to.insert(f.sym, method.sym);
        methods.push(method);
    }
    Ok(methods)
}

/// The indices of `functions`, each after the functions in it.
fn post_order(functions: &[Function]) -> Vec<usize> {
    fn visit(functions: &[Function], i: Option<usize>, out: &mut Vec<usize>) {
        for j in (0..functions.len()).filter(|&j| functions[j].parent == i) {
            visit(functions, Some(j), out);
            out.push(j);
        }
    }
    let mut out = Vec::new();
    visit(functions, None, &mut out);
    out
}

/// The value parameters with their types and the result type of function `f`.
fn signature(l: &mut Lower, f: &Function) -> Result<(Vec<(SymbolId, TypeId)>, TypeId)> {
    let mut result = value_info(l, f.sym);
    if let Type::ByName(r) = *l.u.types().get(result) {
        result = r;
    }
    while let Type::Method { result: r, .. } = *l.u.types().get(result) {
        result = r;
    }
    let params = params(l, f.def).into_iter().map(|p| (p, l.u.info(p))).collect();
    Ok((params, result))
}

/// The type of local method `m` past its type parameters.
pub fn value_info(l: &mut Lower, m: SymbolId) -> TypeId {
    let info = l.u.info(m);
    match *l.u.types().get(info) {
        Type::Poly { result, .. } => result,
        _ => info,
    }
}

/// Lifts named `methods` of `functions` of `class` of definition `td`, inner functions first,
/// each lambda's followed by its `$adapted` method, and the proxies of the `$adapted` methods, to
/// name last in the unit; the lambdas' blocks become closures, the local methods leave their
/// blocks and calls of them in `td` call the lifted methods.
pub fn lift(
    l: &mut Lower,
    (td, class): (TNodeId, SymbolId),
    functions: &[Function],
    methods: &[Method],
) -> Result<(Vec<TNodeId>, Vec<SymbolId>)> {
    let mut plans: Vec<Option<Plan>> = Vec::new();
    let mut deferred = Vec::new();
    for f in functions {
        let (params, result) = signature(l, f)?;
        let prefix = prefix(l, class, functions, methods, f);
        let closure = f.closure.map(|_| plan(l, class, f, &prefix, &params, result, &mut deferred));
        plans.push(closure.transpose()?);
    }
    super::lifted_calls::calls(l, (td, class), functions, methods)?;
    for ((f, method), plan) in functions.iter().zip(methods).zip(&plans) {
        match plan {
            Some(plan) => rewrite(l, f, method, plan),
            None => leave_block(l, f),
        }
    }
    let mut out = Vec::new();
    for i in post_order(functions) {
        let f = &functions[i];
        let Tree::DefDef { tpt, rhs: Some(rhs), .. } = *l.tpd.tree(f.def) else { unreachable!("a function") };
        let map: FxHashMap<SymbolId, SymbolId> =
            f.captured.iter().copied().zip(methods[i].proxies.iter().copied()).collect();
        substitute(l, rhs, &map);
        retarget_returns(l, rhs, f.sym, methods[i].sym);
        out.push(def(l, f.def, &methods[i], tpt, rhs));
        if let Some(adapted) = plans[i].as_ref().and_then(|p| p.adapted.as_ref()) {
            let closure = f.closure.unwrap_or(f.def);
            out.push(super::adapted::adapted_def(l, closure, &methods[i], adapted, tpt));
        }
    }
    Ok((out, deferred))
}

/// A private method `name` of `class`, static unless `this`, with proxies of `captured`, then
/// `params` by name and type; the proxies named as the values they stand for, until
/// `name_proxies`.
pub fn define(
    l: &mut Lower,
    class: SymbolId,
    name: &str,
    captured: &[SymbolId],
    params: &[(String, TypeId)],
    result: TypeId,
    this: bool,
) -> Method {
    let flags = Flags::default().with(PRIVATE);
    let flags = if this { flags } else { flags.with(STATIC) };
    let sym = l.u.define(symbol(name, Kind::Def, class, flags));
    let mut proxies = Vec::new();
    for &s in captured {
        let name = l.u.symbol(s).name.clone();
        let proxy = l.u.define(symbol(&name, Kind::Param, sym, Flags::default()));
        let info = l.u.info(s);
        l.u.set_info(proxy, info);
        proxies.push(proxy);
    }
    let mut typed: Vec<(String, TypeId)> =
        proxies.iter().map(|&p| (l.u.symbol(p).name.clone(), l.u.info(p))).collect();
    typed.extend(params.iter().cloned());
    let binder = l.u.fresh_binder();
    let info =
        l.u.intern_type(Type::Method { binder, params: typed, result, implicit: false, contextual: false });
    l.u.set_info(sym, info);
    Method { sym, proxies }
}

/// Removes local method `f` from the block it is in.
fn leave_block(l: &mut Lower, f: &Function) {
    let Tree::Block { stats, expr } = *l.tpd.tree(f.block) else { unreachable!("a block") };
    let kept: Vec<TNodeId> = l.tpd.items(stats).iter().copied().filter(|&s| s != f.def).collect();
    let stats = l.tpd.list(&kept);
    l.tpd.get_mut(f.block).tree = Tree::Block { stats, expr };
}

/// The value parameters of method definition `def`.
fn params(l: &Lower, def: TNodeId) -> Vec<SymbolId> {
    let Tree::DefDef { paramss, .. } = *l.tpd.tree(def) else { return Vec::new() };
    let clauses = l.tpd.clause_items(paramss);
    let params = clauses.iter().flat_map(|&c| l.tpd.items(c));
    params.filter(|&&p| matches!(l.tpd.tree(p), Tree::ValDef { .. })).filter_map(|&p| l.tpd.sym(p)).collect()
}

/// Makes the references to the keys of `map` under `t`, outside class definitions, refer to their
/// values.
pub fn substitute(l: &mut Lower, t: TNodeId, map: &FxHashMap<SymbolId, SymbolId>) {
    let mut todo = vec![t];
    while let Some(t) = todo.pop() {
        if !matches!(l.tpd.tree(t), Tree::TypeDef { .. }) {
            todo.extend(children(l.tpd, t));
        }
        if let (Tree::Ident { .. }, Some(&proxy)) = (l.tpd.tree(t), l.tpd.sym(t).and_then(|s| map.get(&s))) {
            let name = l.u.symbol(proxy).name.clone();
            let name = l.term_name(&name);
            let tpe = type_ref(l.u, proxy);
            let node = l.tpd.get_mut(t);
            (node.tree, node.sym, node.tpe) = (Tree::Ident { name }, Some(proxy), Some(tpe));
        }
    }
}

/// Makes the returns from `function` under `t` return from `method`.
fn retarget_returns(l: &mut Lower, t: TNodeId, function: SymbolId, method: SymbolId) {
    for t in super::walk::subtrees(l.tpd, t) {
        if let Tree::Return { from, .. } = *l.tpd.tree(t)
            && l.tpd.sym(from) == Some(function)
        {
            let tpe = type_ref(l.u, method);
            (l.tpd.get_mut(from).sym, l.tpd.get_mut(from).tpe) = (Some(method), Some(tpe));
        }
    }
}

/// The definition of `method` from function definition `function`: its proxies, then its
/// parameters.
fn def(l: &mut Lower, function: TNodeId, method: &Method, tpt: TNodeId, rhs: TNodeId) -> TNodeId {
    let span = l.tpd.span(function);
    let mut defs: Vec<TNodeId> = method.proxies.iter().map(|&p| l.param_def(p, span)).collect();
    let Tree::DefDef { paramss, .. } = *l.tpd.tree(function) else { unreachable!("a function") };
    let originals: Vec<TNodeId> =
        l.tpd.clause_items(paramss).iter().flat_map(|&c| l.tpd.items(c).to_vec()).collect();
    defs.extend(originals.into_iter().filter(|&p| matches!(l.tpd.tree(p), Tree::ValDef { .. })));
    l.method_def(method.sym, &defs, tpt, rhs, span)
}
