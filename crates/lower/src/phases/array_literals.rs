//! The array part of dotty's `ArrayApply`: `Array(xs*)` with a class tag of a class literal, and
//! `Array(x, xs*)`, become array literals.

use super::array_apply::{applied, qualifier_symbol, wrapped_literal};
use crate::Lower;
use syntax::ast::Span;
use typer::tpd::{TNodeId, Tree};

/// The array literal for `Array.apply` call `t`, if it applies one.
pub fn array_literal(l: &mut Lower, t: TNodeId) -> Option<TNodeId> {
    let Tree::Apply { args, .. } = *l.tpd.tree(t) else { return None };
    let (m, qual) = applied(l, t)?;
    let module = qualifier_symbol(l, qual).filter(|&q| l.u.is_module(q))?;
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
            let all = l.tpd.node(Tree::SeqLiteral { elems, elemtpt }, Span::NONE, Some(tpe), None);
            Some(l.positioned(all))
        }
        _ => None,
    }
}

/// Whether `t` is `ClassTag.apply(classOf[C])`, its argument cast or not, or a `ClassTag` of a
/// primitive (dotty's `elideClassTag`).
fn is_class_tag_of_literal(l: &Lower, t: TNodeId) -> bool {
    let t = l.tpd.strip_typed(t);
    let Some(m) = l.tpd.sym(l.tpd.meth_part(t)) else { return false };
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
    let &[mut runtime_class] = l.tpd.items(args) else { return false };
    while let Tree::TypeApply { fun, .. } = *l.tpd.tree(runtime_class)
        && l.tpd.sym(fun) == Some(l.defs.as_instance_of)
        && let Tree::Select { qualifier, .. } = *l.tpd.tree(fun)
    {
        runtime_class = qualifier;
    }
    match *l.tpd.tree(runtime_class) {
        Tree::ClassLiteral { .. } => true,
        Tree::TypeApply { fun, .. } => l.tpd.sym(fun).is_some_and(|s| l.u.symbol(s).name == "classOf"),
        Tree::Ident { .. } => l.tpd.sym(runtime_class).is_some_and(|s| l.u.symbol(s).name == "classOf"),
        _ => false,
    }
}
