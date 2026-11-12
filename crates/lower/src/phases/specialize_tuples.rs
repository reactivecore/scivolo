// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/SpecializeTuples.scala
//! dotty's `SpecializeTuples`: an elideable `Tuple2.apply[Int, Int](a, b)` or
//! `new Tuple2[Int, Int](a, b)` creates a `Tuple2$mcII$sp`, and `t._1` of a `Tuple2[Int, Int]`
//! calls `t._1$mcI$sp`; likewise for the other specialized element types and `Tuple1`.

use super::adapted::behaves_as_scala2;
use super::elideable::is_elideable;
use super::walk::walk_class_up;
use crate::definitions::{Known, specialized_accessor, specialized_tuple_ctor, tuple_tags};
use crate::{Lower, Result, unsupported};
use names::{name_kinds, nme};
use rustc_hash::FxHashSet;
use std::ops::ControlFlow;
use syntax::ast::Span;
use tasty::{SymbolId, TypeId};
use typer::tpd::{TNodeId, Tree, Walk};

/// Specializes the tuple creations and element selections in class definition `td`; `inits`
/// are the unit's objects with initialization code; nothing in a file behaving as the Scala 2
/// library.
pub fn transform(l: &mut Lower, td: TNodeId, inits: &FxHashSet<SymbolId>) -> Result<()> {
    if l.tpd.sym(td).is_some_and(|c| behaves_as_scala2(l, c)) {
        return Ok(());
    }
    let (mut result, mut specialized) = (Ok(()), FxHashSet::default());
    walk_class_up(l, td, &mut |_, _, _| Walk::Children, &mut |l, t, _| {
        let s = &mut specialized;
        let done = match *l.tpd.tree(t) {
            Tree::Apply { fun, .. } => creation(l, t, fun, inits, s),
            Tree::Select { qualifier, .. } => selection(l, t, qualifier),
            Tree::Block { expr, .. } => retype(l, t, &[expr], s),
            Tree::If { thenp, elsep, .. } => retype(l, t, &[thenp, elsep], s),
            _ => Ok(()),
        };
        match done {
            Ok(()) => ControlFlow::Continue(()),
            Err(e) => {
                result = Err(e);
                ControlFlow::Break(())
            }
        }
    });
    result
}

/// The `Tuple1` or `Tuple2` class symbol whose companion's `apply` or whose constructor `s` is.
fn tuple_of(l: &mut Lower, s: SymbolId) -> Option<SymbolId> {
    let owner = l.u.symbol(s).owner?;
    let class = match l.u.symbol(s).name.as_str() {
        nme::APPLY if l.u.is_module_class(owner) => l.u.companion_class(owner)?,
        nme::CONSTRUCTOR => owner,
        _ => return None,
    };
    matches!(l.u.full_name(class).as_str(), "scala.Tuple1" | "scala.Tuple2").then_some(class)
}

/// The type tags of the specialized class of a tuple of element types `elems`, if any.
fn tags(l: &mut Lower, elems: &[TypeId]) -> Option<String> {
    let names: Option<Vec<String>> = elems
        .iter()
        .map(|&e| {
            let widened = l.u.widen(e);
            l.u.class_symbol(widened).map(|c| l.u.full_name(c))
        })
        .collect();
    tuple_tags(&names?)
}

/// Makes `t`, `fun(args)`, create the specialized tuple class if `fun` is the `apply` or
/// constructor of `Tuple1` or `Tuple2` applied to specialized element types and `t` is
/// elideable.
fn creation(
    l: &mut Lower,
    t: TNodeId,
    fun: TNodeId,
    inits: &FxHashSet<SymbolId>,
    specialized: &mut FxHashSet<TNodeId>,
) -> Result<()> {
    let Tree::TypeApply { fun: name_tree, args: targs } = *l.tpd.tree(fun) else { return Ok(()) };
    if !matches!(l.tpd.tree(name_tree), Tree::Ident { .. } | Tree::Select { .. }) {
        return Ok(());
    }
    let Some(s) = l.tpd.sym(name_tree) else { return Ok(()) };
    if tuple_of(l, s).is_none() {
        return Ok(());
    }
    let elems: Vec<TypeId> = l.tpd.items(targs).iter().map(|&a| l.tpd.tpe(a)).collect();
    let Some(tags) = tags(l, &elems) else { return Ok(()) };
    if !is_elideable(l, t, inits) {
        return Ok(());
    }
    let ctor = specialized_tuple_ctor(l.u, &tags).or_else(|e| unsupported(l.tpd.span(t), e))?;
    let class = l.u.symbol(ctor).owner.expect("a constructor's class");
    let new = l.new_(class, Span::NONE);
    let select = l.select(new, ctor, Span::NONE);
    let Tree::Apply { args, .. } = *l.tpd.tree(t) else { unreachable!("an application") };
    let tpe = l.tpd.tpe(new);
    let node = l.tpd.get_mut(t);
    (node.tree, node.tpe) = (Tree::Apply { fun: select, args }, Some(tpe));
    specialized.insert(t);
    Ok(())
}

/// Gives `t`, a block or `if` all of whose `results` create one specialized tuple class, that
/// class's type.
fn retype(
    l: &mut Lower,
    t: TNodeId,
    results: &[TNodeId],
    specialized: &mut FxHashSet<TNodeId>,
) -> Result<()> {
    let tpes: Vec<TypeId> = results.iter().map(|&r| l.tpd.tpe(r)).collect();
    if results.iter().all(|r| specialized.contains(r)) && tpes.windows(2).all(|w| w[0] == w[1]) {
        l.tpd.get_mut(t).tpe = Some(tpes[0]);
        specialized.insert(t);
    }
    Ok(())
}

/// Makes `t`, `qual._1` or `qual._2` of a specialized tuple type, select the specialized
/// accessor.
fn selection(l: &mut Lower, t: TNodeId, qual: TNodeId) -> Result<()> {
    let Some(s) = l.tpd.sym(t) else { return Ok(()) };
    let field = l.u.symbol(s).name.clone();
    if field != nme::SELECTOR_1 && field != nme::SELECTOR_2 {
        return Ok(());
    }
    let tpe = l.u.widen(l.tpd.tpe(qual));
    let tpe = l.u.dealias(tpe);
    let Some((tycon, elems)) = l.u.applied(tpe).map(|(c, a)| (c, a.to_vec())) else { return Ok(()) };
    let Some(class) = l.u.class_symbol(tycon) else { return Ok(()) };
    let n = elems.len();
    let base = match n {
        1 => Known::Tuple1,
        2 => Known::Tuple2,
        _ => return Ok(()),
    };
    if !l.u.linearization(class).into_iter().any(|b| l.is_known(b, base)) {
        return Ok(());
    }
    let Some(tags) = tags(l, &elems) else { return Ok(()) };
    let i = if field == nme::SELECTOR_1 { 0 } else { 1 };
    let tag = tags.chars().nth(i).expect("a tag per element");
    let accessor = specialized_accessor(l.u, n, &field, tag).or_else(|e| unsupported(l.tpd.span(t), e))?;
    let name = l.term_name(&name_kinds::specialized(&field, &tag.to_string()));
    let node = l.tpd.get_mut(t);
    (node.tree, node.sym, node.tpe) =
        (Tree::Select { qualifier: qual, name }, Some(accessor), Some(elems[i]));
    Ok(())
}
