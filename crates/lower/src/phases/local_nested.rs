//! The definitions in a local class that the lowering does not lift with it.

use super::local_free::is_term;
use crate::Lower;
use crate::symbols::has;
use tasty::SymbolId;
use tasty::tags::LAZY;
use typer::tpd::{TNodeId, Tree};

/// What local class definition `td` defines that the lowering does not lift with it: local
/// methods and lambdas other than those using the class's `this`, lazy values of an object.
pub fn nested_definition(l: &Lower, td: TNodeId) -> Option<&'static str> {
    let terms = super::explicit_outer::terms(l, td, true);
    let class = l.tpd.sym(td);
    let uses_this = |d: TNodeId| {
        super::walk::subtrees(l.tpd, d)
            .into_iter()
            .any(|t| matches!(l.tpd.tree(t), Tree::This { .. }) && l.tpd.sym(t) == class)
    };
    let closures: Vec<SymbolId> = terms
        .iter()
        .filter_map(|&t| match *l.tpd.tree(t) {
            Tree::Closure { meth, .. } => l.tpd.sym(meth),
            _ => None,
        })
        .collect();
    let lifted: Vec<SymbolId> = terms
        .iter()
        .filter(|&&t| matches!(l.tpd.tree(t), Tree::DefDef { .. }) && uses_this(t))
        .filter_map(|&t| l.tpd.sym(t).filter(|s| closures.contains(s)))
        .collect();
    terms.into_iter().find_map(|t| match *l.tpd.tree(t) {
        Tree::Closure { meth, .. } if l.tpd.sym(meth).is_none_or(|s| !lifted.contains(&s)) => {
            Some("lambdas and local methods in local classes")
        }
        Tree::ValDef { .. } | Tree::DefDef { .. }
            if l.tpd
                .sym(t)
                .is_some_and(|s| has(l.u, s, LAZY) && !l.u.is_module(s) && l.u.symbol(s).owner == class)
                && class.is_some_and(|c| l.u.is_module_class(c)) =>
        {
            Some("lazy values in local objects")
        }
        Tree::DefDef { .. }
            if l.tpd.sym(t).is_some_and(|s| {
                !lifted.contains(&s) && is_term(l, l.u.symbol(s).owner.expect("an owner"))
            }) =>
        {
            Some("lambdas and local methods in local classes")
        }
        _ => None,
    })
}
