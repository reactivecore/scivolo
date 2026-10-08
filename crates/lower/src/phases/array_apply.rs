//! dotty's `ArrayApply`: array literals for `Array(..)` (`array_literals.rs`); `List(xs*)` and
//! `Seq(xs*)` become `::` cells ending in `Nil`, within a budget of 8 elements over nested calls.

use super::adapt::adapt;
use super::array_literals::array_literal;
use super::casts::cast;
use super::walk::children;
use crate::Lower;
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use tasty::{Erased, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Rewrites the array and sequence constructions in class definitions `classes`; lifted lambdas
/// and local classes start with the budget left where they are made.
pub fn transform(l: &mut Lower, classes: &[TNodeId]) {
    let mut budgets = FxHashMap::default();
    loop {
        let known = budgets.clone();
        for &td in classes {
            walk(l, td, &mut budgets);
        }
        if budgets == known {
            break;
        }
    }
    for &td in classes {
        for (t, budget) in walk(l, td, &mut budgets).into_iter().rev() {
            if let Some(elems) = seq_elems(l, t).filter(|e| budget > 0 || e.is_empty()) {
                cons(l, t, &elems);
            } else if let Some(literal) = array_literal(l, t) {
                l.copy_node(t, literal);
            }
        }
    }
}

/// The trees of class definition `td` in pre-order, each with the budget left in it; records in
/// `budgets` the budget where each lambda and local class is made.
fn walk(l: &Lower, td: TNodeId, budgets: &mut FxHashMap<SymbolId, i32>) -> Vec<(TNodeId, i32)> {
    let start = l.tpd.sym(td).and_then(|c| budgets.get(&c).copied()).unwrap_or(BUDGET);
    let mut order = Vec::new();
    let mut todo = vec![(td, start)];
    while let Some((t, budget)) = todo.pop() {
        let budget = match *l.tpd.tree(t) {
            Tree::DefDef { .. } => l.tpd.sym(t).and_then(|m| budgets.get(&m).copied()).unwrap_or(budget),
            _ => budget,
        };
        let budget = budget - seq_elems(l, t).map_or(0, |e| e.len() as i32);
        match *l.tpd.tree(t) {
            Tree::Closure { meth, .. } => {
                budgets.extend(l.tpd.sym(meth).map(|m| (m, budget)));
            }
            Tree::New { tpt } => {
                if let Type::Sym(_, c) = *l.u.types().get(l.tpd.tpe(tpt))
                    && l.local_classes.contains(&c)
                {
                    budgets.insert(c, budget);
                }
            }
            _ => {}
        }
        order.push((t, budget));
        let nested = matches!(l.tpd.tree(t), Tree::TypeDef { .. }) && t != td;
        if !nested {
            todo.extend(children(l.tpd, t).into_iter().map(|c| (c, budget)));
        }
    }
    order
}

/// dotty's `transformListApplyBudget`.
const BUDGET: i32 = 8;

/// The objects whose `apply` builds a list (dotty's `isSeqApply`), and the aliases of `scala`'s
/// package object.
const SEQ_MODULES: [&str; 5] = [
    "scala.collection.immutable.List",
    "scala.collection.immutable.Seq",
    "scala.collection.Seq",
    "scala.package$.List",
    "scala.package$.Seq",
];

/// The method `t` applies, and the receiver of the innermost application.
pub fn applied(l: &Lower, t: TNodeId) -> Option<(SymbolId, TNodeId)> {
    let mut fun = t;
    loop {
        match *l.tpd.tree(fun) {
            Tree::Apply { fun: f, .. } | Tree::TypeApply { fun: f, .. } => fun = f,
            Tree::Select { qualifier, .. } => return Some((l.tpd.sym(fun)?, qualifier)),
            _ => return None,
        }
    }
}

/// The elements of `List(xs*)` or `Seq(xs*)` call `t`.
fn seq_elems(l: &Lower, t: TNodeId) -> Option<Vec<TNodeId>> {
    let Tree::Apply { args, .. } = *l.tpd.tree(t) else { return None };
    let (m, qual) = applied(l, t)?;
    let full = l.u.full_name(qualifier_symbol(l, qual)?);
    if l.u.symbol(m).name != "apply" || !SEQ_MODULES.contains(&full.as_str()) {
        return None;
    }
    let &[arg] = l.tpd.items(args) else { return None };
    let literal = wrapped_literal(l, arg)?;
    let Tree::SeqLiteral { elems, .. } = *l.tpd.tree(literal) else { return None };
    Some(l.tpd.items(elems).to_vec())
}

/// The symbol receiver `qual` refers to, through a getter call.
pub fn qualifier_symbol(l: &Lower, qual: TNodeId) -> Option<SymbolId> {
    let qual = strip_typed(l, qual);
    match *l.tpd.tree(qual) {
        Tree::Apply { fun, args } if l.tpd.items(args).is_empty() => l.tpd.sym(fun),
        _ => l.tpd.sym(qual),
    }
}

/// The array literal a `ScalaRunTime.wrapXArray` call `t` wraps (dotty's `WrapArrayMethods`),
/// or a `genericWrapArray` call an empty one.
pub fn wrapped_literal(l: &Lower, t: TNodeId) -> Option<TNodeId> {
    let t = strip_typed(l, t);
    let Tree::Apply { fun, args } = *l.tpd.tree(t) else { return None };
    let m = l.tpd.sym(fun)?;
    let owner = l.u.symbol(m).owner?;
    if l.u.full_name(owner) != "scala.runtime.ScalaRunTime$" {
        return None;
    }
    let &[literal] = l.tpd.items(args) else { return None };
    let literal = strip_typed(l, literal);
    let Tree::SeqLiteral { elems, .. } = *l.tpd.tree(literal) else { return None };
    let name = &l.u.symbol(m).name;
    let wrap = name.starts_with("wrap") && name.ends_with("Array");
    let generic = name == "genericWrapArray" && l.tpd.items(elems).is_empty();
    (wrap || generic).then_some(literal)
}

pub fn strip_typed(l: &Lower, mut t: TNodeId) -> TNodeId {
    while let Tree::Typed { expr, .. } = *l.tpd.tree(t) {
        t = expr;
    }
    t
}

/// `new ::(e1, ... new ::(en, Nil))` for `List(e1, ..., en)` call `t`, cast to its erased result.
fn cons(l: &mut Lower, t: TNodeId, elems: &[TNodeId]) {
    let defs = l.defs.sequences;
    let object = Erased::Class("java.lang.Object".into());
    let mut acc = l.ident(defs.nil, Span::NONE);
    for &e in elems.iter().rev() {
        let e = adapt(l, e, &object);
        let e = l.positioned(e);
        let new = l.new_(defs.cons, Span::NONE);
        let ctor = l.select(new, defs.cons_ctor, Span::NONE);
        let tpe = l.tpd.tpe(new);
        let apply = l.apply(ctor, &[e, acc], tpe, Span::NONE);
        acc = l.positioned(apply);
    }
    let (m, _) = applied(l, t).expect("a sequence call");
    let result = l.u.erased_method(m).1;
    let consed = cast(l, acc, &result);
    let consed = l.positioned(consed);
    l.copy_node(t, consed);
    l.explicit_casts.insert(t);
}
