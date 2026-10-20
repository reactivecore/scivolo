// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/TupleOptimizations.scala
//   compiler/src/dotty/tools/dotc/core/TypeUtils.scala
//! dotty's `TupleOptimizations` for `Tuples.apply(tup, n)` of a constant `n` and a tuple of known
//! element types: `tup._n+1` through `TupleN`, or `tup.productElement(n)` through `TupleXXL`.

use super::walk::walk_class;
use crate::Lower;
use tasty::tags::INTconst;
use tasty::{Type, TypeId};
use typer::tpd::{TNodeId, Tree, Walk};

/// The largest arity of a `TupleN` class.
const MAX_TUPLE_ARITY: usize = 22;

/// Rewrites the calls of `Tuples.apply` in class definition `td`.
pub fn transform(l: &mut Lower, td: TNodeId) {
    let mut calls = Vec::new();
    walk_class(l, td, &mut |l, t, _| {
        if is_tuples_apply(l, t) {
            calls.push(t);
        }
        Walk::Children
    });
    for t in calls {
        tuple_apply(l, t);
    }
}

fn is_tuples_apply(l: &mut Lower, t: TNodeId) -> bool {
    let Tree::Apply { fun, .. } = *l.tpd.tree(t) else { return false };
    let Some(s) = l.tpd.sym(fun) else { return false };
    let owner = l.u.symbol(s).owner;
    l.u.symbol(s).name == "apply" && owner.is_some_and(|o| l.u.full_name(o) == "scala.runtime.Tuples$")
}

/// Replaces call `t`, `Tuples.apply(tup, n)`, by the selection of element `n` if `tup`'s element
/// types are known (dotty's `transformTupleApply`), keeping `t`'s type.
fn tuple_apply(l: &mut Lower, t: TNodeId) {
    let Tree::Apply { args, .. } = *l.tpd.tree(t) else { return };
    let &[tup, index] = l.tpd.items(args) else { return };
    let index_type = l.tpd.tpe(index);
    let Type::Constant(tag, n) = *l.u.types().get(index_type) else { return };
    if tag != INTconst {
        return;
    }
    let tup_type = l.tpd.tpe(tup);
    let Some(elems) = tuple_element_types(l, tup_type) else { return };
    let Some(n) = usize::try_from(n).ok().filter(|&n| n < elems.len()) else { return };
    let element = if elems.len() <= MAX_TUPLE_ARITY {
        let class = l.u.class_ref(&format!("scala.Tuple{}", elems.len()));
        let tuple = l.u.intern_type(Type::Applied(class, elems));
        let Some(selector) = member(l, tuple, &format!("_{}", n + 1)) else { return };
        let value = l.ensure_conforms(tup, tuple);
        l.select(value, selector, syntax::ast::Span::NONE)
    } else {
        let xxl = l.u.class_ref("scala.runtime.TupleXXL");
        let Some(product_element) = member(l, xxl, "productElement") else { return };
        let value = l.ensure_conforms(tup, xxl);
        let fun = l.select(value, product_element, syntax::ast::Span::NONE);
        let fun = l.positioned(fun);
        let tpe = l.tpd.tpe(t);
        l.apply(fun, &[index], tpe, syntax::ast::Span::NONE)
    };
    let element = l.positioned(element);
    let tpe = l.tpd.get(t).tpe;
    l.copy_node(t, element);
    l.tpd.get_mut(t).tpe = tpe;
}

/// The member of class type `tpe` named `name`.
fn member(l: &mut Lower, tpe: TypeId, name: &str) -> Option<tasty::SymbolId> {
    let class = l.u.class_symbol(tpe)?;
    l.u.decl(class, name).ok()?.first().copied()
}

/// The element types of tuple type `t` if they are known: a `*:` chain ending in `EmptyTuple`, or
/// a `TupleN` (dotty's `tupleElementTypes`).
pub fn tuple_element_types(l: &mut Lower, t: TypeId) -> Option<Vec<TypeId>> {
    let widened = l.u.widen(t);
    let t = l.u.dealias(widened);
    if let Type::Applied(tycon, args) = l.u.types().get(t).clone() {
        let name = l.u.class_symbol(tycon).map(|c| l.u.full_name(c))?;
        if name == "scala.*:" && args.len() == 2 {
            let mut out = vec![args[0]];
            out.extend(tuple_element_types(l, args[1])?);
            return Some(out);
        }
        return (name == format!("scala.Tuple{}", args.len())).then_some(args);
    }
    let class = match *l.u.types().get(t) {
        Type::Sym(_, s) if l.u.is_module(s) => l.u.module_class(s),
        _ => l.u.class_symbol(t),
    };
    let empty = class.is_some_and(|c| l.u.full_name(c).trim_end_matches('$').ends_with(".EmptyTuple"));
    empty.then(Vec::new)
}
