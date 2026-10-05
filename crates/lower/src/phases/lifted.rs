//! The methods lifted from lambdas: named `<owner>$$anonfun$N` from the outermost lambda in,
//! taking proxies `x$N` of the captured values first; an `$adapted` method where a parameter is
//! primitive or the result `Unit` (dotty's `Erasure.Typer.typedClosure`).

use super::adapt::from_start;
use super::lambda_lift::{Lambda, fresh, post_order};
use super::walk::children;
use crate::definitions::{procedure, symbol, type_ref};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashMap;
use tasty::tags::{PRIVATE, STATIC};
use tasty::{Erased, Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// A lambda's method: its symbol and the proxies of the captured values.
pub struct Method {
    pub sym: SymbolId,
    pub proxies: Vec<SymbolId>,
}

/// The methods of `lambdas` of `class`, inner lambdas first, each followed by its `$adapted`
/// method, and the proxies of the `$adapted` methods, named last in the unit; the lambdas'
/// blocks become closures of them.
pub fn lift(l: &mut Lower, class: SymbolId, lambdas: &[Lambda]) -> Result<(Vec<TNodeId>, Vec<SymbolId>)> {
    let mut lifted: Vec<(Method, Option<Method>)> = Vec::new();
    let mut procedures = Vec::new();
    let mut deferred = Vec::new();
    for lambda in lambdas {
        let owner = match lambda.parent {
            Some(p) => l.u.symbol(lifted[p].0.sym).name.clone(),
            None => lambda.method.clone(),
        };
        let params = params(l, lambda.def);
        let info = l.u.info(lambda.sym);
        let Type::Method { result, .. } = *l.u.types().get(info) else { unreachable!("a method type") };
        let name = fresh(l, &format!("{owner}$$anonfun"));
        let typed: Vec<(SymbolId, TypeId)> = params.iter().map(|&p| (p, l.u.info(p))).collect();
        let method = define(l, class, &name, &lambda.captured, &typed, result, true);
        let erased: Vec<Erased> = typed.iter().map(|&(_, t)| l.u.erase(t)).collect();
        let erased_result = l.u.erase(result);
        let unit = erased_result == Erased::Unit;
        let primitive = erased.iter().any(|e| matches!(e, Erased::Prim(_) | Erased::Unit));
        let function = matches!(l.tpd.tree(lambda.closure), Tree::Closure { tpt: None, .. });
        if function && (unit || primitive) && super::adapted::specializable(&erased, &erased_result) {
            return unsupported(l.tpd.span(lambda.block), "lambdas of specialized function types");
        }
        procedures.push((function && unit && !primitive).then_some(params.len()));
        let adapted = (primitive || unit && !function).then(|| {
            let object = type_ref(l.u, l.defs.object);
            let objects: Vec<(SymbolId, TypeId)> = params.iter().map(|&p| (p, object)).collect();
            let result = if unit { object } else { result };
            let name = fresh(l, &format!("{owner}$$anonfun$adapted"));
            let adapted = define(l, class, &name, &lambda.captured, &objects, result, false);
            l.bridges.insert(adapted.sym);
            deferred.extend(adapted.proxies.iter().copied());
            adapted
        });
        lifted.push((method, adapted));
    }
    for ((lambda, (method, adapted)), void_arity) in lambdas.iter().zip(&lifted).zip(procedures) {
        let span = l.tpd.span(lambda.closure);
        let env: Vec<TNodeId> = lambda.captured.iter().map(|&s| l.ident(s, span)).collect();
        let target = adapted.as_ref().unwrap_or(method).sym;
        let meth = l.ident(target, span);
        let Tree::Closure { tpt, .. } = *l.tpd.tree(lambda.closure) else { unreachable!("a closure") };
        let tpt = match void_arity {
            Some(arity) => {
                let Ok(interface) = procedure(l.u, arity) else {
                    return unsupported(span, format!("lambdas of {arity} parameters returning Unit"));
                };
                let interface = type_ref(l.u, interface);
                Some(l.type_tree(interface, span))
            }
            None => tpt,
        };
        let env = l.tpd.list(&env);
        l.tpd.get_mut(lambda.closure).tree = Tree::Closure { env, meth, tpt };
        let stats = l.tpd.list(&[]);
        let expr = match adapted {
            Some(_) => {
                let closure = l.tpd.get_mut(lambda.closure);
                let span = closure.span;
                closure.span = from_start(span);
                l.block(&[], lambda.closure, span)
            }
            None => lambda.closure,
        };
        l.tpd.get_mut(lambda.block).tree = Tree::Block { stats, expr };
    }
    let mut out = Vec::new();
    for i in post_order(lambdas) {
        let (method, adapted) = &lifted[i];
        let Tree::DefDef { tpt, rhs: Some(rhs), .. } = *l.tpd.tree(lambdas[i].def) else {
            unreachable!("a lambda")
        };
        let map: FxHashMap<SymbolId, SymbolId> =
            lambdas[i].captured.iter().copied().zip(method.proxies.iter().copied()).collect();
        substitute(l, rhs, &map);
        let params = params(l, lambdas[i].def);
        out.push(def(l, lambdas[i].def, method, &params, tpt, rhs));
        if let Some(adapted) = adapted {
            out.push(super::adapted::adapted_def(l, lambdas[i].def, method, adapted, tpt));
        }
    }
    Ok((out, deferred))
}

/// A private static method `name` of `class` with proxies of `captured`, then `params`; the
/// proxies named `x$N` now if `named`, else later by `name_proxies`.
fn define(
    l: &mut Lower,
    class: SymbolId,
    name: &str,
    captured: &[SymbolId],
    params: &[(SymbolId, TypeId)],
    result: TypeId,
    named: bool,
) -> Method {
    let sym = l.u.define(symbol(name, Kind::Def, class, Flags::default().with(PRIVATE).with(STATIC)));
    let mut proxies = Vec::new();
    for &s in captured {
        let base = l.u.symbol(s).name.clone();
        let name = if named { fresh(l, &base) } else { base };
        let proxy = l.u.define(symbol(&name, Kind::Param, sym, Flags::default()));
        let info = l.u.info(s);
        l.u.set_info(proxy, info);
        proxies.push(proxy);
    }
    let mut typed: Vec<(String, TypeId)> =
        proxies.iter().map(|&p| (l.u.symbol(p).name.clone(), l.u.info(p))).collect();
    typed.extend(params.iter().map(|&(p, t)| (l.u.symbol(p).name.clone(), t)));
    let binder = l.u.fresh_binder();
    let info = l.u.intern_type(Type::Method { binder, params: typed, result, implicit: false });
    l.u.set_info(sym, info);
    Method { sym, proxies }
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

/// The definition of `method` with `params` after its proxies, from lambda definition `lambda`.
fn def(
    l: &mut Lower,
    lambda: TNodeId,
    method: &Method,
    params: &[SymbolId],
    tpt: TNodeId,
    rhs: TNodeId,
) -> TNodeId {
    let span = l.tpd.span(lambda);
    let mut defs: Vec<TNodeId> = method.proxies.iter().map(|&p| l.param_def(p, span)).collect();
    let Tree::DefDef { paramss, .. } = *l.tpd.tree(lambda) else { unreachable!("a lambda") };
    let originals: Vec<TNodeId> =
        l.tpd.clause_items(paramss).iter().flat_map(|&c| l.tpd.items(c).to_vec()).collect();
    defs.extend(originals.into_iter().filter(|&p| l.tpd.sym(p).is_some_and(|s| params.contains(&s))));
    l.method_def(method.sym, &defs, tpt, rhs, span)
}
