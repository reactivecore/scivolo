//! Lambdas (dotty's `LambdaLift` for closures): each `{ def $anonfun(ps) = body; closure }` of a
//! class's methods becomes a private static method of the class taking the captured values
//! first, and the closure passes them.

use super::walk::children;
use crate::symbols::{has, is_class, is_module_class};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashSet;
use tasty::tags::{FIELDaccessor, MUTABLE};
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// A lambda: its block and `$anonfun` definition, the lambda it is in, the name of the method
/// it is in, the values it captures.
pub struct Lambda {
    pub block: TNodeId,
    pub def: TNodeId,
    pub closure: TNodeId,
    pub sym: SymbolId,
    pub parent: Option<usize>,
    pub method: String,
    pub captured: Vec<SymbolId>,
}

/// Lifts the lambdas of class definitions `classes` into methods of their classes; the proxies
/// of `$adapted` methods are named last.
pub fn transform(l: &mut Lower, classes: &[TNodeId]) -> Result<()> {
    let mut deferred = Vec::new();
    for &td in classes {
        deferred.extend(lift_class(l, td)?);
    }
    for p in deferred {
        let base = l.u.symbol(p).name.clone();
        let name = fresh(l, &base);
        l.u.rename(p, &name);
    }
    Ok(())
}

/// `base$N`, `N` counting the names made from `base` in the unit.
pub fn fresh(l: &mut Lower, base: &str) -> String {
    let n = l.lifted_names.entry(base.to_owned()).or_insert(0);
    *n += 1;
    format!("{base}${n}")
}

/// The indices of `lambdas`, each after the lambdas in it.
pub fn post_order(lambdas: &[Lambda]) -> Vec<usize> {
    fn visit(lambdas: &[Lambda], i: Option<usize>, out: &mut Vec<usize>) {
        for j in (0..lambdas.len()).filter(|&j| lambdas[j].parent == i) {
            visit(lambdas, Some(j), out);
            out.push(j);
        }
    }
    let mut out = Vec::new();
    visit(lambdas, None, &mut out);
    out
}

/// Lifts the lambdas of class definition `td`; the proxies to name last.
fn lift_class(l: &mut Lower, td: TNodeId) -> Result<Vec<SymbolId>> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else {
        return Ok(Vec::new());
    };
    let mut stats = l.tpd.items(body).to_vec();
    let mut lambdas = Vec::new();
    for &stat in &stats {
        let method = match (l.tpd.tree(stat), l.tpd.sym(stat)) {
            (Tree::DefDef { .. }, Some(s)) if !has(l.u, s, FIELDaccessor) => l.u.symbol(s).name.clone(),
            _ => String::new(),
        };
        let first = lambdas.len();
        collect(l, stat, None, &method, &mut lambdas);
        if method.is_empty() && lambdas.len() > first {
            return unsupported(l.tpd.span(lambdas[first].block), "lambdas outside methods");
        }
    }
    if lambdas.is_empty() {
        return Ok(Vec::new());
    }
    for i in 0..lambdas.len() {
        lambdas[i].captured = captured(l, &lambdas, i, class)?;
    }
    let (lifted, deferred) = super::lifted::lift(l, class, &lambdas)?;
    stats.extend(lifted);
    let body = l.tpd.list(&stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(deferred)
}

/// The lambdas under `t` in pre-order, `parent` the one they are in.
fn collect(l: &Lower, t: TNodeId, parent: Option<usize>, method: &str, out: &mut Vec<Lambda>) {
    if let Some((def, closure, sym)) = lambda(l, t) {
        out.push(Lambda { block: t, def, closure, sym, parent, method: method.into(), captured: Vec::new() });
        let index = out.len() - 1;
        if let Tree::DefDef { rhs: Some(rhs), .. } = *l.tpd.tree(def) {
            collect(l, rhs, Some(index), method, out);
        }
        return;
    }
    if matches!(l.tpd.tree(t), Tree::TypeDef { .. }) {
        return;
    }
    for c in children(l.tpd, t) {
        collect(l, c, parent, method, out);
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

/// The values lambda `i` captures, in the order its body first refers to them: the local values
/// defined outside it.
fn captured(l: &Lower, lambdas: &[Lambda], i: usize, class: SymbolId) -> Result<Vec<SymbolId>> {
    let inside: FxHashSet<SymbolId> = (0..lambdas.len())
        .filter(|&j| std::iter::successors(Some(j), |&k| lambdas[k].parent).any(|k| k == i))
        .map(|j| lambdas[j].sym)
        .collect();
    let mut out = Vec::new();
    let mut todo = vec![lambdas[i].def];
    while let Some(t) = todo.pop() {
        todo.extend(children(l.tpd, t).into_iter().rev());
        let span = l.tpd.span(t);
        match (l.tpd.tree(t), l.tpd.sym(t)) {
            (Tree::This { .. }, _) => return unsupported(span, "lambdas using `this`"),
            (Tree::Ident { .. }, Some(s)) => {
                let owner = l.u.symbol(s).owner;
                if owner == Some(class) && !is_module_class(l.u, class) {
                    return unsupported(span, "lambdas using `this` of a class");
                }
                let local = owner.is_some_and(|o| l.u.symbol(o).kind == Kind::Def && !is_class(l.u, o));
                if local && !inside.contains(&owner.expect("an owner")) && !out.contains(&s) {
                    if has(l.u, s, MUTABLE) {
                        return unsupported(span, "lambdas capturing variables");
                    }
                    out.push(s);
                }
            }
            _ => {}
        }
    }
    Ok(out)
}
