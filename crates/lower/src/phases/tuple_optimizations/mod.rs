// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/TupleOptimizations.scala
//! dotty's `TupleOptimizations`: the calls of `scala.runtime.Tuples` on tuples of known element
//! types made into `TupleN` constructions, selections and iterations.

pub(super) mod elements;
mod known;

use self::elements::{MAX_TUPLE_ARITY, tuple_element_types, tuple_element_types_up_to, widen_term_ref_expr};
use self::known::{Once, empty_tuple, from_elements, from_iterator, method, module_member};
use self::known::{next, select_applied, selectors, tuple_type, typed};
use super::first_transform::literal;
use super::walk::{children, walk_class_up, without_type_args};
use crate::Lower;
use crate::definitions::type_ref;
use std::ops::ControlFlow;
use syntax::ast::Span;
use tasty::tags::INTconst;
use tasty::{SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree, Walk};

/// The most elements the tuples built of selected elements have.
const MAX_SELECTED: usize = 5;

/// Rewrites the calls of `scala.runtime.Tuples` in class definition `td`, inner calls first.
pub fn transform(l: &mut Lower, td: TNodeId) {
    let mut calls = Vec::new();
    let mut leave = |l: &mut Lower, t, owner| {
        if let Some(m) = tuples_method(l, t) {
            calls.push((t, owner, m));
        }
        ControlFlow::Continue(())
    };
    walk_class_up(l, td, &mut |_, _, _| Walk::Children, &mut leave);
    for (t, owner, m) in calls {
        let name = l.u.symbol(m).name.clone();
        let first_new = l.tpd.len();
        let mut kept = Vec::new();
        let Some(optimized) = optimized(l, t, owner, &name, &mut kept) else { continue };
        let span = l.tpd.span(t);
        if span.is_inlined_call() {
            respan(l, optimized, first_new, &kept, span);
        }
        l.copy_node(t, optimized);
    }
}

/// Gives the trees made from node `first_new` on in `t`, outside the copies `kept`, span `span`
/// of the inline call they replace a tree of, as trees inlined from another source have it.
fn respan(l: &mut Lower, t: TNodeId, first_new: usize, kept: &[TNodeId], span: Span) {
    if t.index() < first_new || kept.contains(&t) {
        return;
    }
    l.tpd.get_mut(t).span = span;
    for c in children(l.tpd, t) {
        respan(l, c, first_new, kept, span);
    }
}

/// The method of `scala.runtime.Tuples` call `t` calls.
fn tuples_method(l: &mut Lower, t: TNodeId) -> Option<SymbolId> {
    let Tree::Apply { fun, .. } = *l.tpd.tree(t) else { return None };
    let s = l.tpd.sym(without_type_args(l.tpd, fun))?;
    let owner = l.u.symbol(s).owner?;
    (l.u.full_name(owner) == "scala.runtime.Tuples$").then_some(s)
}

/// The tree call `t` of `Tuples.name` in `owner` becomes, if any; the copies of its arguments it
/// holds go to `kept`.
fn optimized(
    l: &mut Lower,
    t: TNodeId,
    owner: SymbolId,
    name: &str,
    kept: &mut Vec<TNodeId>,
) -> Option<TNodeId> {
    let Tree::Apply { args, .. } = *l.tpd.tree(t) else { return None };
    match (name, l.tpd.items(args).to_vec().as_slice()) {
        ("cons", &[head, tail]) => cons(l, t, owner, head, tail, kept),
        ("tail", &[tup]) => tuple_tail(l, t, owner, tup, kept),
        ("size", _) => size(l, t),
        ("concat", &[a, b]) => concat(l, t, owner, a, b, kept),
        ("apply", &[tup, n]) => apply(l, t, tup, n),
        ("toArray", &[tup]) => to_array(l, t, tup),
        _ => None,
    }
}

/// The element types of the tuple `t` is of, through references to terms.
fn elements(l: &mut Lower, t: TNodeId) -> Option<Vec<TypeId>> {
    let tpe = l.tpd.tpe(t);
    let widened = widen_term_ref_expr(l, tpe);
    tuple_element_types(l, widened)
}

/// `TupleN+1(head, t._1, …, t._n)` of `val t = tail`, or of more elements
/// `TupleN+1(it.next(), …)` of `val it = Tuples.consIterator(head, tail)` (dotty's
/// `transformTupleCons`); none of more than 22 elements.
fn cons(
    l: &mut Lower,
    t: TNodeId,
    owner: SymbolId,
    head: TNodeId,
    tail: TNodeId,
    kept: &mut Vec<TNodeId>,
) -> Option<TNodeId> {
    let tpes = elements(l, t)?;
    let size = tpes.len();
    if size == 0 || size > MAX_TUPLE_ARITY {
        return None;
    }
    if size > MAX_SELECTED {
        let it = tuples_call(l, "consIterator", &[head, tail])?;
        return from_iterator_cast(l, it, size, l.tpd.tpe(t), owner);
    }
    let tail = if size > 1 { typed(l, tail, &tpes[1..])? } else { tail };
    let mut tup = Once::new(l, tail, owner);
    let mut elements = vec![head];
    elements.extend(selectors(l, &mut tup, size - 1, 0..size - 1)?);
    let tuple = from_elements(l, &tpes, &elements)?;
    Some(tup.wrap(l, tuple, kept))
}

/// `EmptyTuple`, `TupleN-1(t._2, …, t._n)` of `val t = tup`, or the elements after the first of
/// `tup.productIterator` (dotty's `transformTupleTail`); none of more than 22 elements.
fn tuple_tail(
    l: &mut Lower,
    t: TNodeId,
    owner: SymbolId,
    tup: TNodeId,
    kept: &mut Vec<TNodeId>,
) -> Option<TNodeId> {
    let tpe = l.tpd.tpe(tup);
    let widened = widen_term_ref_expr(l, tpe);
    let tpes = tuple_element_types_up_to(l, widened, MAX_TUPLE_ARITY)?;
    let size = tpes.len();
    if size == 0 || size > MAX_TUPLE_ARITY {
        None
    } else if size == 1 {
        empty_tuple(l)
    } else if size <= MAX_SELECTED {
        let typed = typed(l, tup, &tpes)?;
        let mut tup = Once::new(l, typed, owner);
        let elements = selectors(l, &mut tup, size, 1..size)?;
        let tuple = from_elements(l, &tpes[1..], &elements)?;
        Some(tup.wrap(l, tuple, kept))
    } else {
        let product = l.product().ok()?;
        let product_type = type_ref(l.u, product);
        let product_iterator = method(l, product, "productIterator")?;
        let qual = l.ensure_conforms(tup, product_type);
        let iterator = select_applied(l, qual, product_iterator);
        let mut it = Once::new(l, iterator, owner);
        let first = next(l, &mut it)?;
        let rest = from_iterator(l, size - 1, &mut it)?;
        let rest = l.ensure_conforms(rest, l.tpd.tpe(t));
        let block = l.block(&[first], rest, Span::NONE);
        let block = l.positioned(block);
        Some(it.wrap(l, block, kept))
    }
}

/// The size of a tuple type that normalizes to a constant (dotty's `transformTupleSize`).
fn size(l: &mut Lower, t: TNodeId) -> Option<TNodeId> {
    let tpe = l.tpd.tpe(t);
    let constant = l.u.try_normalize(tpe)?;
    let tree = literal(l.u.types().get(constant))?;
    Some(l.tpd.node(tree, Span::NONE, Some(constant), None))
}

/// `a` or `b` if the other is empty, `TupleN+M(t._1, …, t._n, u._1, …, u._m)` of `val t = a`
/// and `val u = b`, or of more elements those of `Tuples.concatIterator(a, b)` (dotty's
/// `transformTupleConcat`); none of more than 22 elements.
fn concat(
    l: &mut Lower,
    t: TNodeId,
    owner: SymbolId,
    a: TNodeId,
    b: TNodeId,
    kept: &mut Vec<TNodeId>,
) -> Option<TNodeId> {
    let (tpes1, tpes2) = (elements(l, a)?, elements(l, b)?);
    let (n, m) = (tpes1.len(), tpes2.len());
    if n == 0 {
        return Some(b);
    }
    if m == 0 {
        return Some(a);
    }
    if n + m > MAX_TUPLE_ARITY {
        return None;
    }
    if n + m >= MAX_SELECTED {
        let it = tuples_call(l, "concatIterator", &[a, b])?;
        return from_iterator_cast(l, it, n + m, l.tpd.tpe(t), owner);
    }
    let typed_a = typed(l, a, &tpes1)?;
    let mut first = Once::new(l, typed_a, owner);
    let typed_b = typed(l, b, &tpes2)?;
    let mut second = Once::new(l, typed_b, owner);
    let mut elements = selectors(l, &mut first, n, 0..n)?;
    elements.extend(selectors(l, &mut second, m, 0..m)?);
    let tuple = from_elements(l, &[tpes1, tpes2].concat(), &elements)?;
    let inner = second.wrap(l, tuple, kept);
    Some(first.wrap(l, inner, kept))
}

/// `tup._n+1` through `TupleN` for a constant `n`, of `t`'s type (dotty's `transformTupleApply`);
/// none of more than 22 elements.
fn apply(l: &mut Lower, t: TNodeId, tup: TNodeId, index: TNodeId) -> Option<TNodeId> {
    let index_type = l.tpd.tpe(index);
    let Type::Constant(tag, n) = *l.u.types().get(index_type) else { return None };
    if tag != INTconst {
        return None;
    }
    let tpes = elements(l, tup)?;
    let n = usize::try_from(n).ok().filter(|&n| n < tpes.len())?;
    let tuple = tuple_type(l, &tpes)?;
    let class = l.u.class_symbol(tuple)?;
    let selector = method(l, class, &format!("_{}", n + 1))?;
    let value = l.ensure_conforms(tup, tuple);
    let element = l.select(value, selector, Span::NONE);
    let element = l.positioned(element);
    l.tpd.get_mut(element).tpe = l.tpd.get(t).tpe;
    Some(element)
}

/// `Array.emptyObjectArray` or `Tuples.productToArray(tup)` (dotty's `transformTupleToArray`);
/// none of more than 22 elements.
fn to_array(l: &mut Lower, t: TNodeId, tup: TNodeId) -> Option<TNodeId> {
    let tpe = l.tpd.tpe(tup);
    let widened = l.u.widen(tpe);
    let size = tuple_element_types_up_to(l, widened, MAX_TUPLE_ARITY)?.len();
    if size == 0 {
        let (array, empty) = module_member(l, "scala", "Array", "emptyObjectArray")?;
        let qual = l.ident(array, Span::NONE);
        let empty = select_applied(l, qual, empty);
        let span = l.tpd.span(t);
        Some(l.with_span(empty, span))
    } else if size <= MAX_TUPLE_ARITY {
        let product = l.product().ok()?;
        let product_type = type_ref(l.u, product);
        let arg = l.ensure_conforms(tup, product_type);
        tuples_call(l, "productToArray", &[arg])
    } else {
        None
    }
}

/// The tuple of `size` elements of iterator `it`, as `tpe` (dotty's `evalOnce` of
/// `knownTupleFromIterator`).
fn from_iterator_cast(
    l: &mut Lower,
    it: TNodeId,
    size: usize,
    tpe: TypeId,
    owner: SymbolId,
) -> Option<TNodeId> {
    let mut it = Once::new(l, it, owner);
    let tuple = from_iterator(l, size, &mut it)?;
    let tuple = l.ensure_conforms(tuple, tpe);
    Some(it.wrap(l, tuple, &mut Vec::new()))
}

/// `Tuples.name(args)`.
fn tuples_call(l: &mut Lower, name: &str, args: &[TNodeId]) -> Option<TNodeId> {
    let (tuples, m) = module_member(l, "scala.runtime", "Tuples", name)?;
    let qual = l.ident(tuples, Span::NONE);
    Some(l.call(qual, m, args))
}
