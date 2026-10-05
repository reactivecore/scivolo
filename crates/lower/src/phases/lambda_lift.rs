//! Lambdas and local methods (dotty's `LambdaLift`): each `{ def $anonfun(ps) = body; closure }`
//! and each `def f(ps) = body` in a block of a class's methods becomes a private method of the
//! class taking the captured values first; closures and calls pass them.

use super::captures::{direct, propagate};
use super::walk::children;
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use tasty::SymbolId;
use tasty::tags::FIELDaccessor;
use typer::tpd::{TNodeId, Tree};

/// A function to lift: its definition, the block it is in, its closure if a lambda, the function
/// it is in, the values it captures, whether it needs `this`.
pub struct Function {
    pub def: TNodeId,
    pub block: TNodeId,
    pub closure: Option<TNodeId>,
    pub sym: SymbolId,
    pub parent: Option<usize>,
    pub captured: Vec<SymbolId>,
    pub this: bool,
}

/// Lifts the lambdas and local methods of class definitions `classes` into methods of their
/// classes; the proxies of `$adapted` methods are named last.
pub fn transform(l: &mut Lower, classes: &[TNodeId]) -> Result<()> {
    let mut deferred = Vec::new();
    for &td in classes {
        deferred.extend(lift_class(l, td)?);
    }
    name_proxies(l, &deferred);
    Ok(())
}

/// Names proxies `proxies`, each `x$N` for the value `x` it stands for.
pub fn name_proxies(l: &mut Lower, proxies: &[SymbolId]) {
    for &p in proxies {
        let base = l.u.symbol(p).name.clone();
        let name = fresh(l, &base);
        l.u.rename(p, &name);
    }
}

/// `base$N`, `N` counting the names made from `base` in the unit.
pub fn fresh(l: &mut Lower, base: &str) -> String {
    let n = l.lifted_names.entry(base.to_owned()).or_insert(0);
    *n += 1;
    format!("{base}${n}")
}

/// The indices of `functions`, each after the functions in it.
pub fn post_order(functions: &[Function]) -> Vec<usize> {
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

/// Lifts the functions of class definition `td`; the proxies to name last.
fn lift_class(l: &mut Lower, td: TNodeId) -> Result<Vec<SymbolId>> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else {
        return Ok(Vec::new());
    };
    let mut stats = l.tpd.items(body).to_vec();
    let mut functions = Vec::new();
    for &stat in &stats {
        let method = matches!((l.tpd.tree(stat), l.tpd.sym(stat)),
            (Tree::DefDef { .. }, Some(s)) if !has(l.u, s, FIELDaccessor));
        let first = functions.len();
        match *l.tpd.tree(stat) {
            Tree::DefDef { rhs: Some(rhs), .. } if method => collect(l, rhs, None, &mut functions),
            _ => collect(l, stat, None, &mut functions),
        }
        if !method && functions.len() > first {
            return unsupported(
                l.tpd.span(functions[first].def),
                "lambdas and local methods outside methods",
            );
        }
    }
    if functions.is_empty() {
        return Ok(Vec::new());
    }
    let mut calls = Vec::new();
    for i in 0..functions.len() {
        let (captured, called, this) = direct(l, &functions, i, class)?;
        (functions[i].captured, functions[i].this) = (captured, this || functions[i].closure.is_none());
        calls.push(called);
    }
    propagate(l, &mut functions, &calls);
    let (lifted, deferred) = super::lifted::lift(l, class, template, &functions)?;
    stats.extend(lifted);
    let body = l.tpd.list(&stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(deferred)
}

/// The lambdas and local methods under `t` in pre-order, `parent` the one they are in.
fn collect(l: &Lower, t: TNodeId, parent: Option<usize>, out: &mut Vec<Function>) {
    if let Some((def, closure, sym)) = lambda(l, t) {
        out.push(Function {
            def,
            block: t,
            closure: Some(closure),
            sym,
            parent,
            captured: Vec::new(),
            this: false,
        });
        let index = out.len() - 1;
        if let Tree::DefDef { rhs: Some(rhs), .. } = *l.tpd.tree(def) {
            collect(l, rhs, Some(index), out);
        }
        return;
    }
    match *l.tpd.tree(t) {
        Tree::TypeDef { .. } => {}
        Tree::Block { stats, expr } => {
            for &s in l.tpd.items(stats) {
                match (l.tpd.tree(s), l.tpd.sym(s)) {
                    (Tree::DefDef { rhs: Some(rhs), .. }, Some(sym)) => {
                        let rhs = *rhs;
                        out.push(Function {
                            def: s,
                            block: t,
                            closure: None,
                            sym,
                            parent,
                            captured: Vec::new(),
                            this: true,
                        });
                        let index = out.len() - 1;
                        collect(l, rhs, Some(index), out);
                    }
                    _ => collect(l, s, parent, out),
                }
            }
            collect(l, expr, parent, out);
        }
        _ => {
            for c in children(l.tpd, t) {
                collect(l, c, parent, out);
            }
        }
    }
}

/// The `$anonfun` definition, closure and method symbol of lambda block `t`.
fn lambda(l: &Lower, t: TNodeId) -> Option<(TNodeId, TNodeId, SymbolId)> {
    let Tree::Block { stats, expr } = *l.tpd.tree(t) else { return None };
    let &[def] = l.tpd.items(stats) else { return None };
    let Tree::Closure { meth, .. } = *l.tpd.tree(expr) else { return None };
    let sym = l.tpd.sym(def)?;
    (matches!(l.tpd.tree(def), Tree::DefDef { .. }) && l.tpd.sym(meth) == Some(sym))
        .then_some((def, expr, sym))
}
