//! dotty's `ArrayApply`: `Array(xs*)` with a class tag of a class literal, and `Array(x, xs*)`,
//! become array literals; `List(xs*)` and `Seq(xs*)` become `::` cells ending in `Nil`, within a
//! budget of 8 elements over nested calls.

use super::adapt::adapt;
use super::casts::cast;
use super::walk::children;
use crate::Lower;
use crate::symbols::is_module;
use syntax::ast::Span;
use tasty::{Erased, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// Rewrites the array and sequence constructions in class definition `td`.
pub fn transform(l: &mut Lower, td: TNodeId) {
    let mut order = Vec::new();
    let mut todo = vec![(td, 8i32)];
    while let Some((t, budget)) = todo.pop() {
        let budget = budget - seq_elems(l, t).map_or(0, |e| e.len() as i32);
        order.push((t, budget));
        let nested = matches!(l.tpd.tree(t), Tree::TypeDef { .. }) && t != td;
        if !nested {
            todo.extend(children(l.tpd, t).into_iter().map(|c| (c, budget)));
        }
    }
    for (t, budget) in order.into_iter().rev() {
        if let Some(elems) = seq_elems(l, t).filter(|e| budget > 0 || e.is_empty()) {
            cons(l, t, &elems);
        } else if let Some(literal) = array_literal(l, t) {
            l.copy_node(t, literal);
        }
    }
}

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
fn applied(l: &Lower, t: TNodeId) -> Option<(SymbolId, TNodeId)> {
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
fn qualifier_symbol(l: &Lower, qual: TNodeId) -> Option<SymbolId> {
    let qual = strip_typed(l, qual);
    match *l.tpd.tree(qual) {
        Tree::Apply { fun, args } if l.tpd.items(args).is_empty() => l.tpd.sym(fun),
        _ => l.tpd.sym(qual),
    }
}

/// The array literal `ScalaRunTime.wrapXArray` call `t` wraps.
fn wrapped_literal(l: &Lower, t: TNodeId) -> Option<TNodeId> {
    let t = strip_typed(l, t);
    let Tree::Apply { fun, args } = *l.tpd.tree(t) else { return None };
    let m = l.tpd.sym(fun)?;
    let owner = l.u.symbol(m).owner?;
    let name = &l.u.symbol(m).name;
    let wrap = name.starts_with("wrap") && name.ends_with("Array") || name == "genericWrapArray";
    if !wrap || l.u.full_name(owner) != "scala.runtime.ScalaRunTime$" {
        return None;
    }
    let &[literal] = l.tpd.items(args) else { return None };
    let literal = strip_typed(l, literal);
    matches!(l.tpd.tree(literal), Tree::SeqLiteral { .. }).then_some(literal)
}

fn strip_typed(l: &Lower, mut t: TNodeId) -> TNodeId {
    while let Tree::Typed { expr, .. } = *l.tpd.tree(t) {
        t = expr;
    }
    t
}

/// `new ::(e1, ... new ::(en, Nil))` for `List(e1, ..., en)` call `t`, cast to its erased result.
fn cons(l: &mut Lower, t: TNodeId, elems: &[TNodeId]) {
    let defs = l.defs.array_apply;
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

/// The array literal for `Array.apply` call `t`, if it applies one.
fn array_literal(l: &mut Lower, t: TNodeId) -> Option<TNodeId> {
    let Tree::Apply { args, .. } = *l.tpd.tree(t) else { return None };
    let (m, qual) = applied(l, t)?;
    let module = qualifier_symbol(l, qual).filter(|&q| is_module(l.u, q))?;
    if l.u.symbol(m).name != "apply" || l.u.full_name(module) != "scala.Array" {
        return None;
    }
    match *l.tpd.items(args) {
        [seq, tag] if is_class_tag_of_literal(l, tag) => wrapped_literal(l, seq),
        [first, seq] => {
            let literal = wrapped_literal(l, seq)?;
            let Tree::SeqLiteral { elems, elemtpt } = *l.tpd.tree(literal) else { return None };
            let all: Vec<TNodeId> =
                std::iter::once(first).chain(l.tpd.items(elems).iter().copied()).collect();
            let elems = l.tpd.list(&all);
            let tpe = l.tpd.tpe(literal);
            Some(l.tpd.node(Tree::SeqLiteral { elems, elemtpt }, l.tpd.span(literal), Some(tpe), None))
        }
        _ => None,
    }
}

/// Whether `t` is `ClassTag.apply(classOf[C])` or a `ClassTag` of a primitive (dotty's
/// `elideClassTag`).
fn is_class_tag_of_literal(l: &Lower, t: TNodeId) -> bool {
    let t = strip_typed(l, t);
    let Some((m, _)) = applied(l, t).or_else(|| Some((l.tpd.sym(t)?, t))) else { return false };
    let Some(owner) = l.u.symbol(m).owner else { return false };
    if l.u.full_name(owner) != "scala.reflect.ClassTag$" {
        return false;
    }
    let name = l.u.symbol(m).name.as_str();
    if name != "apply" {
        return ["Byte", "Short", "Char", "Int", "Long", "Float", "Double", "Boolean", "Unit"]
            .contains(&name);
    }
    let Tree::Apply { args, .. } = *l.tpd.tree(t) else { return false };
    let &[runtime_class] = l.tpd.items(args) else { return false };
    match *l.tpd.tree(runtime_class) {
        Tree::ClassLiteral { .. } => true,
        Tree::TypeApply { fun, .. } => l.tpd.sym(fun).is_some_and(|s| l.u.symbol(s).name == "classOf"),
        _ => false,
    }
}
