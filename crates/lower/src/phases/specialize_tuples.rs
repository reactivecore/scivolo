//! dotty's `SpecializeTuples`: an elideable `Tuple2.apply[Int, Int](a, b)` or
//! `new Tuple2[Int, Int](a, b)` creates a `Tuple2$mcII$sp`, and `t._1` of a `Tuple2[Int, Int]`
//! calls `t._1$mcI$sp`; likewise for the other specialized element types and `Tuple1`.

use super::elideable::is_elideable;
use super::walk::walk_class_up;
use crate::definitions::{specialized_accessor, specialized_tuple_ctor, tuple_tags};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashSet;
use std::ops::ControlFlow;
use syntax::ast::Span;
use tasty::{SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree, Walk};

/// Specializes the tuple creations and element selections in class definition `td`; `inits`
/// are the unit's objects with initialization code.
pub fn transform(l: &mut Lower, td: TNodeId, inits: &FxHashSet<SymbolId>) -> Result<()> {
    let mut result = Ok(());
    walk_class_up(l, td, &mut |_, _, _| Walk::Children, &mut |l, t, _| {
        let done = match *l.tpd.tree(t) {
            Tree::Apply { fun, .. } => creation(l, t, fun, inits),
            Tree::Select { qualifier, .. } => selection(l, t, qualifier),
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
        "apply" if l.u.is_module_class(owner) => l.u.companion_class(owner)?,
        "<init>" => owner,
        _ => return None,
    };
    matches!(l.u.full_name(class).as_str(), "scala.Tuple1" | "scala.Tuple2").then_some(class)
}

/// The type tags of the specialized class of a tuple of element types `elems`, if any.
fn tags(l: &mut Lower, elems: &[TypeId]) -> Option<String> {
    let names: Option<Vec<String>> =
        elems.iter().map(|&e| l.u.class_symbol(e).map(|c| l.u.full_name(c))).collect();
    tuple_tags(&names?)
}

/// Makes `t`, `fun(args)`, create the specialized tuple class if `fun` is the `apply` or
/// constructor of `Tuple1` or `Tuple2` applied to specialized element types and `t` is
/// elideable.
fn creation(l: &mut Lower, t: TNodeId, fun: TNodeId, inits: &FxHashSet<SymbolId>) -> Result<()> {
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
    let ctor = specialized_tuple_ctor(l.u, &tags, &elems).or_else(|e| unsupported(l.tpd.span(t), e))?;
    let class = l.u.symbol(ctor).owner.expect("a constructor's class");
    let new = l.new_(class, Span::NONE);
    let select = l.select(new, ctor, Span::NONE);
    let Tree::Apply { args, .. } = *l.tpd.tree(t) else { unreachable!("an application") };
    l.tpd.get_mut(t).tree = Tree::Apply { fun: select, args };
    Ok(())
}

/// Makes `t`, `qual._1` or `qual._2` of a specialized tuple type, select the specialized
/// accessor.
fn selection(l: &mut Lower, t: TNodeId, qual: TNodeId) -> Result<()> {
    let Some(s) = l.tpd.sym(t) else { return Ok(()) };
    let field = l.u.symbol(s).name.clone();
    if field != "_1" && field != "_2" {
        return Ok(());
    }
    let tpe = l.u.widen(l.tpd.tpe(qual));
    let tpe = l.u.dealias(tpe);
    let Type::Applied(tycon, elems) = l.u.types().get(tpe).clone() else { return Ok(()) };
    let Some(class) = l.u.class_symbol(tycon) else { return Ok(()) };
    let n = elems.len();
    let base = format!("scala.Tuple{n}");
    if (n != 1 && n != 2) || !l.u.linearization(class).iter().any(|&b| l.u.full_name(b) == base) {
        return Ok(());
    }
    let Some(tags) = tags(l, &elems) else { return Ok(()) };
    let i = if field == "_1" { 0 } else { 1 };
    let tag = tags.chars().nth(i).expect("a tag per element");
    let accessor =
        specialized_accessor(l.u, n, &field, tag, elems[i]).or_else(|e| unsupported(l.tpd.span(t), e))?;
    let name = l.term_name(&format!("{field}$mc{tag}$sp"));
    let node = l.tpd.get_mut(t);
    (node.tree, node.sym, node.tpe) =
        (Tree::Select { qualifier: qual, name }, Some(accessor), Some(elems[i]));
    Ok(())
}
