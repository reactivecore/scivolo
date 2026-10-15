//! The functions lifted into a class enclosing the one they are defined in (dotty's
//! `narrowLogicOwner`): those reading a value captured by an enclosing local class, and those of a
//! local trait using a value from outside it, lifted into the outermost local trait between.

use super::lambda_lift::Function;
use super::local_free::{enclosing_class, is_term, original_owner, top_level, within};
use super::walk::subtrees;
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashMap;
use tasty::SymbolId;

/// The class function `f` of `class` becomes a method of: the class it is narrowed into, `class`
/// if it needs `this`, else its top-level class (dotty's `logicOwner`).
pub fn home(l: &Lower, class: SymbolId, f: &Function) -> SymbolId {
    match f.into {
        Some(c) => c,
        None if f.this => class,
        None => top_level(l, class),
    }
}

/// Sets the class each of `functions` of `class` is narrowed into, before calls carry it.
pub fn narrow(l: &Lower, class: SymbolId, functions: &mut [Function]) {
    let proxies: FxHashMap<SymbolId, SymbolId> = l
        .trait_methods
        .iter()
        .filter(|&(&m, _)| l.u.symbol(m).owner == Some(class))
        .flat_map(|(_, (_, pairs))| pairs.iter().map(|&(v, p)| (p, v)))
        .collect();
    let traits = enclosing_traits(l, class);
    for f in functions {
        let values =
            subtrees(l.tpd, f.def).into_iter().filter_map(|t| l.tpd.sym(t).and_then(|s| proxies.get(&s)));
        let outermost = |&v: &SymbolId| traits.iter().rposition(|&t| !within(l, v, t)).unwrap_or(0);
        let into = values.map(outermost).min().map(|i| traits[i]);
        f.into = l.narrowed.get(&f.sym).copied().or(into);
    }
}

/// `class` and the local traits it is in before lifting, innermost first.
fn enclosing_traits(l: &Lower, class: SymbolId) -> Vec<SymbolId> {
    let mut out = vec![class];
    while let Some(owner) = original_owner(l, out[out.len() - 1]).filter(|&o| is_term(l, o)) {
        let enclosing = enclosing_class(l, owner);
        if !l.u.is_trait(enclosing) || !l.local_classes.contains_key(&enclosing) {
            break;
        }
        out.push(enclosing);
    }
    out
}

/// Keeps in `class` the functions narrowed into an enclosing class only through calls that need
/// `this`; fails where a function narrowed itself needs `this`, or a lambda is narrowed but not
/// where it is created.
pub fn check(l: &Lower, class: SymbolId, functions: &mut [Function]) -> Result<()> {
    let outside = |f: &Function| f.into.is_some_and(|c| c != class);
    for f in functions.iter_mut().filter(|f| f.this && outside(f)) {
        if l.narrowed.contains_key(&f.sym) {
            return unsupported(l.tpd.span(f.def), "local functions using two enclosing instances");
        }
        f.into = None;
    }
    for f in functions.iter().filter(|f| f.closure.is_some() && outside(f)) {
        if f.parent.is_none_or(|p| functions[p].into != f.into) {
            return unsupported(l.tpd.span(f.def), "lambdas reading values of enclosing local classes");
        }
    }
    Ok(())
}
