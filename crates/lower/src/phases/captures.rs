//! What lifted functions capture (dotty's `Dependencies`): the local values free in them or in the
//! functions they call, in the order dotty records them, and whether they need `this`.

use super::lambda_lift::Function;
use super::walk::children;
use crate::symbols::{has, is_class, is_module_class};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashMap;
use tasty::tags::MUTABLE;
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// Sets the captured values and `this` of `functions` of `class`, whose methods have the bodies
/// `roots`; the functions in the order they first captured a value, those capturing one
/// directly first, and their number.
pub fn dependencies(
    l: &Lower,
    functions: &mut [Function],
    class: SymbolId,
    roots: &[TNodeId],
) -> Result<(Vec<usize>, usize)> {
    let defs: FxHashMap<TNodeId, usize> = functions.iter().enumerate().map(|(i, f)| (f.def, i)).collect();
    let (mut order, mut called) = (Vec::new(), Vec::<(usize, Vec<usize>)>::new());
    let mut todo: Vec<(TNodeId, Option<usize>)> = roots.iter().rev().map(|&r| (r, None)).collect();
    while let Some((t, enclosure)) = todo.pop() {
        let enclosure = defs.get(&t).copied().or(enclosure);
        if !matches!(l.tpd.tree(t), Tree::TypeDef { .. }) {
            todo.extend(children(l.tpd, t).into_iter().rev().map(|c| (c, enclosure)));
        }
        let Some(i) = enclosure else { continue };
        match (l.tpd.tree(t), l.tpd.sym(t)) {
            (Tree::This { .. }, _) => functions[i].this = true,
            (Tree::Ident { .. }, Some(s)) => {
                if let Some(j) = functions.iter().position(|f| f.sym == s) {
                    match called.iter_mut().find(|(c, _)| *c == i) {
                        Some((_, callees)) if !callees.contains(&j) => callees.push(j),
                        Some(_) => {}
                        None => called.push((i, vec![j])),
                    }
                    continue;
                }
                let owner = l.u.symbol(s).owner;
                functions[i].this |= owner == Some(class) && !is_module_class(l.u, class);
                let local = owner.is_some_and(|o| l.u.symbol(o).kind == Kind::Def && !is_class(l.u, o));
                if local && mark_free(l, functions, &mut order, s, i) && has(l.u, s, MUTABLE) {
                    return unsupported(l.tpd.span(t), "local methods capturing variables");
                }
            }
            _ => {}
        }
    }
    let direct = order.len();
    loop {
        let mut changed = false;
        for &(caller, ref callees) in &called {
            for &callee in callees {
                changed |= functions[callee].this && !functions[caller].this;
                functions[caller].this |= functions[callee].this;
                for s in functions[callee].captured.clone() {
                    changed |= mark_free(l, functions, &mut order, s, caller);
                }
            }
        }
        if !changed {
            return Ok((order, direct));
        }
    }
}

/// Marks local value `s` free in function `i` and the functions it is in, up to the one defining
/// `s`, outermost first; whether that added any.
fn mark_free(l: &Lower, functions: &mut [Function], order: &mut Vec<usize>, s: SymbolId, i: usize) -> bool {
    let owner = l.u.symbol(s).owner;
    let mut chain = Vec::new();
    let mut k = Some(i);
    while let Some(j) = k.filter(|&j| Some(functions[j].sym) != owner) {
        chain.push(j);
        k = functions[j].parent;
    }
    if k.is_none() && functions.iter().any(|f| Some(f.sym) == owner) {
        return false;
    }
    let mut added = false;
    for &j in chain.iter().rev() {
        if functions[j].captured.contains(&s) {
            continue;
        }
        if functions[j].captured.is_empty() {
            order.push(j);
        }
        functions[j].captured.push(s);
        added = true;
    }
    added
}
