// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/TupleOptimizations.scala
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//! The trees of `TupleOptimizations`: tuples of known elements, their selectors, values
//! evaluated once.

use super::elements::MAX_TUPLE_ARITY;
use crate::Lower;
use crate::definitions::applied;
use crate::phases::eval_once::{eval_once, is_stable_path};
use crate::phases::tree_copy::{Copier, copy};
use syntax::ast::Span;
use tasty::{SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// A value evaluated once (dotty's `evalOnce`): the expression itself if it is idempotent, else
/// `val ev$N` of it.
pub struct Once {
    expr: TNodeId,
    owner: SymbolId,
    def: Option<(SymbolId, TNodeId)>,
    copies: Vec<TNodeId>,
}

impl Once {
    pub fn new(l: &mut Lower, expr: TNodeId, owner: SymbolId) -> Once {
        let def = (!is_idempotent(l, expr)).then(|| eval_once(l, expr, owner));
        Once { expr, owner, def, copies: Vec::new() }
    }

    /// A reference to the value.
    pub fn get(&mut self, l: &mut Lower) -> TNodeId {
        if let Some((ev, _)) = self.def {
            return l.ident(ev, Span::NONE);
        }
        let mut copier = Copier { syms: Default::default(), from: self.owner, to: self.owner };
        let copied = copy(l, &mut copier, self.expr).unwrap_or(self.expr);
        self.copies.push(copied);
        copied
    }

    /// `{ val ev$N = expr; t }`, or `t` if the expression is idempotent; adds the copies of the
    /// expression to `kept`.
    pub fn wrap(self, l: &mut Lower, t: TNodeId, kept: &mut Vec<TNodeId>) -> TNodeId {
        kept.extend(self.copies);
        let Some((_, def)) = self.def else { return t };
        let block = l.block(&[def], t, Span::NONE);
        l.positioned(block)
    }
}

/// Whether `t` is a path or a literal, possibly ascribed.
fn is_idempotent(l: &mut Lower, t: TNodeId) -> bool {
    match *l.tpd.tree(t) {
        Tree::Typed { expr, .. } => is_idempotent(l, expr),
        _ => is_stable_path(l, t),
    }
}

/// `expr: TupleN[elems]`.
pub fn typed(l: &mut Lower, expr: TNodeId, elems: &[TypeId]) -> Option<TNodeId> {
    let tpe = tuple_type(l, elems)?;
    let tpt = l.type_tree(tpe, Span::NONE);
    let typed = l.tpd.node(Tree::Typed { expr, tpt }, Span::NONE, Some(tpe), None);
    Some(l.positioned(typed))
}

/// `TupleN[elems]` of 1 to 22 elements.
pub fn tuple_type(l: &mut Lower, elems: &[TypeId]) -> Option<TypeId> {
    let class = tuple_class(l, elems.len())?;
    Some(applied(l.u, class, elems.to_vec()))
}

fn tuple_class(l: &mut Lower, n: usize) -> Option<SymbolId> {
    if n == 0 || n > MAX_TUPLE_ARITY {
        return None;
    }
    let class = l.u.class_ref(&format!("scala.Tuple{n}"));
    l.u.class_symbol(class)
}

/// `tup._i+1` of each `i` of `range`, `tup` a `TupleN` of `n` elements (dotty's
/// `tupleSelectors`).
pub fn selectors(
    l: &mut Lower,
    tup: &mut Once,
    n: usize,
    range: std::ops::Range<usize>,
) -> Option<Vec<TNodeId>> {
    let class = tuple_class(l, n)?;
    let mut out = Vec::new();
    for i in range {
        let selector = method(l, class, &format!("_{}", i + 1))?;
        let qual = tup.get(l);
        let select = l.select(qual, selector, Span::NONE);
        out.push(l.positioned(select));
    }
    Some(out)
}

/// `TupleN.apply[tpes](elements)` (dotty's `knownTupleFromElements`).
pub fn from_elements(l: &mut Lower, tpes: &[TypeId], elements: &[TNodeId]) -> Option<TNodeId> {
    let class = tuple_class(l, tpes.len())?;
    let (module, apply) = module_member(l, "scala", &format!("Tuple{}", tpes.len()), "apply")?;
    let qual = l.ident(module, Span::NONE);
    let fun = l.select(qual, apply, Span::NONE);
    let fun = l.positioned(fun);
    let targs: Vec<TNodeId> = tpes.iter().map(|&t| l.type_tree(t, Span::NONE)).collect();
    let info = l.u.member_info(l.tpd.tpe(qual), apply);
    let fun = l.build().type_apply(fun, &targs, info, Span::NONE);
    let fun = l.positioned(fun);
    let tuple = applied(l.u, class, tpes.to_vec());
    let app = l.apply(fun, elements, tuple, Span::NONE);
    Some(l.positioned(app))
}

/// `TupleN(it.next(), …, it.next())` of 1 to 22 elements (dotty's `knownTupleFromIterator`).
pub fn from_iterator(l: &mut Lower, size: usize, it: &mut Once) -> Option<TNodeId> {
    tuple_class(l, size)?;
    let any = l.u.class_ref("scala.Any");
    let elements = (0..size).map(|_| next(l, it)).collect::<Option<Vec<_>>>()?;
    from_elements(l, &vec![any; size], &elements)
}

/// `it.next()`.
pub fn next(l: &mut Lower, it: &mut Once) -> Option<TNodeId> {
    let next = class_member(l, "scala.collection.Iterator", "next")?;
    let qual = it.get(l);
    Some(l.call(qual, next, &[]))
}

/// A reference to the object `EmptyTuple`.
pub fn empty_tuple(l: &mut Lower) -> Option<TNodeId> {
    let package_object = module(l, "scala", "Tuple$package")?;
    let class = l.u.module_class(package_object)?;
    let empty = l.u.decl(class, "EmptyTuple").ok()?.into_iter().find(|&s| l.u.is_module(s))?;
    Some(l.ident(empty, Span::NONE))
}

/// `qual.m`, applied to no arguments if `m` is a method (dotty's `ensureApplied`).
pub fn select_applied(l: &mut Lower, qual: TNodeId, m: SymbolId) -> TNodeId {
    let info = l.u.info(m);
    if matches!(l.u.types().get(info), Type::Method { .. }) {
        return l.call(qual, m, &[]);
    }
    let select = l.select(qual, m, Span::NONE);
    l.positioned(select)
}

/// The object `name` of package `pkg`.
fn module(l: &mut Lower, pkg: &str, name: &str) -> Option<SymbolId> {
    let pkg = l.u.package(pkg)?;
    l.u.decl(pkg, name).ok()?.into_iter().find(|&s| l.u.is_module(s))
}

/// The object `name` of package `pkg` and its member `m`.
pub fn module_member(l: &mut Lower, pkg: &str, name: &str, m: &str) -> Option<(SymbolId, SymbolId)> {
    let module = module(l, pkg, name)?;
    let class = l.u.module_class(module)?;
    Some((module, method(l, class, m)?))
}

/// The member `m` of the class of qualified name `full`.
pub fn class_member(l: &mut Lower, full: &str, m: &str) -> Option<SymbolId> {
    let tpe = l.u.class_ref(full);
    let class = l.u.class_symbol(tpe)?;
    method(l, class, m)
}

/// The member `name` of class `class`.
pub fn method(l: &mut Lower, class: SymbolId, name: &str) -> Option<SymbolId> {
    l.u.decl(class, name).ok()?.first().copied()
}
