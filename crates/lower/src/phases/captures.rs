//! What lifted functions capture (dotty's `Dependencies`): the local values they refer to that
//! are defined outside them, those of the functions they call, and whether they need `this`.

use super::lambda_lift::Function;
use super::walk::children;
use crate::symbols::{has, is_class, is_module_class};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashSet;
use tasty::tags::MUTABLE;
use tasty::{Kind, SymbolId};
use typer::tpd::Tree;

/// The functions `i` and those in it.
fn inside(functions: &[Function], i: usize) -> FxHashSet<SymbolId> {
    (0..functions.len())
        .filter(|&j| std::iter::successors(Some(j), |&k| functions[k].parent).any(|k| k == i))
        .map(|j| functions[j].sym)
        .collect()
}

/// The values function `i` of `class` refers to that are defined outside it, in the order it
/// first refers to them; the functions outside it it calls; whether it refers to `this`.
pub fn direct(
    l: &Lower,
    functions: &[Function],
    i: usize,
    class: SymbolId,
) -> Result<(Vec<SymbolId>, Vec<usize>, bool)> {
    let inside = inside(functions, i);
    let (mut captured, mut calls, mut this) = (Vec::new(), Vec::new(), false);
    let mut todo = vec![functions[i].def];
    while let Some(t) = todo.pop() {
        todo.extend(children(l.tpd, t).into_iter().rev());
        match (l.tpd.tree(t), l.tpd.sym(t)) {
            (Tree::This { .. }, _) => this = true,
            (Tree::Ident { .. }, Some(s)) => {
                if let Some(j) = functions.iter().position(|f| f.sym == s) {
                    if !inside.contains(&s) && !calls.contains(&j) {
                        calls.push(j);
                    }
                    continue;
                }
                let owner = l.u.symbol(s).owner;
                this |= owner == Some(class) && !is_module_class(l.u, class);
                let local = owner.is_some_and(|o| l.u.symbol(o).kind == Kind::Def && !is_class(l.u, o));
                if local && !inside.contains(&owner.expect("an owner")) && !captured.contains(&s) {
                    if has(l.u, s, MUTABLE) {
                        return unsupported(l.tpd.span(t), "local methods capturing variables");
                    }
                    captured.push(s);
                }
            }
            _ => {}
        }
    }
    Ok((captured, calls, this))
}

/// Adds to each function the values and `this` the functions it calls capture, repeatedly.
pub fn propagate(l: &Lower, functions: &mut [Function], calls: &[Vec<usize>]) {
    loop {
        let mut changed = false;
        for i in 0..functions.len() {
            let inside = inside(functions, i);
            for &j in &calls[i] {
                let added: Vec<SymbolId> = functions[j]
                    .captured
                    .iter()
                    .copied()
                    .filter(|s| !functions[i].captured.contains(s))
                    .filter(|&s| l.u.symbol(s).owner.is_none_or(|o| !inside.contains(&o)))
                    .collect();
                let this = functions[j].this && !functions[i].this;
                changed |= !added.is_empty() || this;
                functions[i].captured.extend(added);
                functions[i].this |= functions[j].this;
            }
        }
        if !changed {
            return;
        }
    }
}
