// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
//! The parents of a class as erasure leaves them: a `*:` parent becomes `Object`.

use crate::definitions::{Known, type_ref};
use crate::{Lower, Result};
use names::nme;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

/// Replaces the parent `T1 *: … *: EmptyTuple` of class definition `td`, which `scala.TupleN`
/// has, by `Object`: in its parents and, as the constructor call `Object()`, in its template
/// (dotty's `eraseParent` in `TypeErasure.eraseInfo`).
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let mut parents = l.u.parents(class);
    let classes: Vec<_> = parents.iter().map(|&p| l.u.class_symbol(p)).collect();
    let Some(k) = classes.into_iter().position(|c| is_pair(l, c)) else { return Ok(()) };
    let object = l.u.defn().object;
    let object_type = type_ref(l.u, object);
    parents[k] = object_type;
    l.u.set_parents(class, parents);

    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return Ok(()) };
    let mut trees = l.tpd.items(parents).to_vec();
    let Some(tree) = trees.iter_mut().find(|t| is_pair(l, constructed_class(l, **t))) else { return Ok(()) };
    let span = l.tpd.span(*tree);
    let ctor = super::super::lazy_trees::method(l, object, nme::CONSTRUCTOR, span)?;
    let new = l.new_(object, span);
    let fun = l.select(new, ctor, span);
    *tree = l.apply(fun, &[], object_type, span);
    let parents = l.tpd.list(&trees);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

/// Whether `c` is the class `*:`.
fn is_pair(l: &Lower, c: Option<SymbolId>) -> bool {
    c.is_some_and(|c| l.is_known(c, Known::Pair))
}

/// The class parent tree `t` calls the constructor of, if it is a constructor call.
fn constructed_class(l: &Lower, t: TNodeId) -> Option<SymbolId> {
    let ctor = l.tpd.sym(l.tpd.strip_apply(t)).filter(|&s| l.u.is_constructor(s))?;
    l.u.symbol(ctor).owner
}
