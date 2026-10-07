//! The methods lifted from lambdas and local methods: named `<owner>$$anonfun$N` and `f$N` from
//! the outermost function in, taking proxies `x$N` of the captured values first; static unless
//! they need `this`. Calls of local methods pass the captured values.

use super::closures::{Plan, plan, rewrite};
use super::lambda_lift::{Function, fresh, post_order};
use super::walk::children;
use crate::definitions::{symbol, type_ref};
use crate::{Lower, Result, unsupported};
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
        methods.push(define(l, class, &name, &f.captured, &params, result, f.this));
    }
    Ok(methods)
}

/// The parameters with their types and the result type of function `f`.
fn signature(l: &mut Lower, f: &Function) -> Result<(Vec<(SymbolId, TypeId)>, TypeId)> {
    let info = l.u.info(f.sym);
    let result = match *l.u.types().get(info) {
        Type::Method { result, .. } => result,
        Type::Poly { .. } => return unsupported(l.tpd.span(f.def), "local methods with type parameters"),
        _ => return unsupported(l.tpd.span(f.def), "local methods without parameter lists"),
    };
    let params = params(l, f.def).into_iter().map(|p| (p, l.u.info(p))).collect();
    Ok((params, result))
}

/// Names `methods` of `functions` of `class`, inner functions first, each lambda's followed by
/// its `$adapted` method, and the proxies of the `$adapted` methods, to name last in the unit;
/// the lambdas' blocks become closures, the local methods leave their blocks and calls of them in
/// `template` call the lifted methods.
pub fn lift(
    l: &mut Lower,
    class: SymbolId,
    template: TNodeId,
    functions: &[Function],
    methods: &[Method],
) -> Result<(Vec<TNodeId>, Vec<SymbolId>)> {
    let mut plans: Vec<Option<Plan>> = Vec::new();
    let mut deferred = Vec::new();
    for (f, method) in functions.iter().zip(methods) {
        let (params, result) = signature(l, f)?;
        let prefix = match l.u.symbol(f.sym).owner {
            Some(o) if l.u.symbol(o).kind == Kind::Def => match functions.iter().position(|x| x.sym == o) {
                Some(p) => format!("{}$", l.u.symbol(methods[p].sym).name),
                None => format!("{}$", l.u.symbol(o).name),
            },
            Some(o) if f.ctor && l.u.symbol(o).owner == Some(class) => "$init$$".to_owned(),
            _ => String::new(),
        };
        let name = match f.closure {
            Some(_) => fresh(l, &format!("{prefix}$anonfun")),
            None => fresh(l, &l.u.symbol(f.sym).name.clone()),
        };
        l.u.rename(method.sym, &name);
        let closure = match f.closure {
            Some(_) => Some(plan(l, class, f, &prefix, &params, result, &mut deferred)?),
            None => None,
        };
        plans.push(closure);
    }
    calls(l, class, template, functions, methods);
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
        out.push(def(l, f.def, &methods[i], tpt, rhs));
        if let Some(adapted) = plans[i].as_ref().and_then(|p| p.adapted.as_ref()) {
            out.push(super::adapted::adapted_def(l, f.def, &methods[i], adapted, tpt));
        }
    }
    Ok((out, deferred))
}

/// A private method `name` of `class`, static unless `this`, with proxies of `captured`, then
/// `params`; the proxies named as the values they stand for, until `name_proxies`.
pub fn define(
    l: &mut Lower,
    class: SymbolId,
    name: &str,
    captured: &[SymbolId],
    params: &[(SymbolId, TypeId)],
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
    typed.extend(params.iter().map(|&(p, t)| (l.u.symbol(p).name.clone(), t)));
    let binder = l.u.fresh_binder();
    let info =
        l.u.intern_type(Type::Method { binder, params: typed, result, implicit: false, contextual: false });
    l.u.set_info(sym, info);
    Method { sym, proxies }
}

/// Makes the calls of the local methods among `functions` under `t` call their `methods` with
/// the captured values first.
fn calls(l: &mut Lower, class: SymbolId, t: TNodeId, functions: &[Function], methods: &[Method]) {
    let mut todo = vec![t];
    while let Some(t) = todo.pop() {
        if let Tree::Apply { fun, args } = *l.tpd.tree(t)
            && let Some(i) = l.tpd.sym(fun).and_then(|s| functions.iter().position(|f| f.sym == s))
            && functions[i].closure.is_none()
            && matches!(l.tpd.tree(fun), Tree::Ident { .. })
        {
            let span = l.tpd.span(fun);
            let this = l.this(class, span);
            let fun = l.select(this, methods[i].sym, span);
            let mut all: Vec<TNodeId> = functions[i].captured.iter().map(|&s| l.ident(s, span)).collect();
            all.extend(l.tpd.items(args));
            let args = l.tpd.list(&all);
            l.tpd.get_mut(t).tree = Tree::Apply { fun, args };
        }
        if !matches!(l.tpd.tree(t), Tree::TypeDef { .. }) {
            todo.extend(children(l.tpd, t));
        }
    }
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
    clauses.iter().flat_map(|&c| l.tpd.items(c)).filter_map(|&p| l.tpd.sym(p)).collect()
}

/// Makes the references to the keys of `map` under `t` refer to their values.
fn substitute(l: &mut Lower, t: TNodeId, map: &FxHashMap<SymbolId, SymbolId>) {
    let mut todo = vec![t];
    while let Some(t) = todo.pop() {
        todo.extend(children(l.tpd, t));
        if let (Tree::Ident { .. }, Some(&proxy)) = (l.tpd.tree(t), l.tpd.sym(t).and_then(|s| map.get(&s))) {
            let name = l.u.symbol(proxy).name.clone();
            let name = l.term_name(&name);
            let tpe = type_ref(l.u, proxy);
            let node = l.tpd.get_mut(t);
            (node.tree, node.sym, node.tpe) = (Tree::Ident { name }, Some(proxy), Some(tpe));
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
