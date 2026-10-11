//! The applications dotty's `BetaReduce` rewrites, which the lowering refuses: `apply` of a
//! function type called on a lambda or on a new anonymous class with only that method.

use super::walk::try_walk_class;
use crate::{Lower, Result, unsupported};
use typer::tpd::{TNodeId, Tree, Walk};

/// Fails for an application in class definition `td` that `BetaReduce` would reduce.
pub fn check(l: &mut Lower, td: TNodeId) -> Result<()> {
    try_walk_class(l, td, &mut |l, t, _| {
        if let Tree::Apply { fun, .. } = *l.tpd.tree(t)
            && let Tree::Select { qualifier, .. } = *l.tpd.tree(fun)
            && l.tpd.sym(fun).is_some_and(|s| l.u.symbol(s).name == "apply")
            && is_function_type(l, qualifier)
            && reducible(l, qualifier)
        {
            return unsupported(l.tpd.span(t), "applications of a lambda or anonymous function class");
        }
        Ok(Walk::Children)
    })
}

/// Whether the type of `t` is a `FunctionN` or `ContextFunctionN` type.
fn is_function_type(l: &mut Lower, t: TNodeId) -> bool {
    let tpe = l.u.widen(l.tpd.tpe(t));
    let Some(class) = l.u.class_symbol(tpe) else { return false };
    let name = l.u.full_name(class);
    let arity = name.strip_prefix("scala.Function").or_else(|| name.strip_prefix("scala.ContextFunction"));
    arity.is_some_and(|n| n.parse::<u32>().is_ok())
}

/// Whether `t`, past blocks and ascriptions, is a lambda or a new anonymous class whose body is
/// one method.
fn reducible(l: &Lower, t: TNodeId) -> bool {
    match *l.tpd.tree(t) {
        Tree::Typed { expr, .. } => reducible(l, expr),
        Tree::TypeApply { fun, .. } => match *l.tpd.tree(fun) {
            Tree::Select { qualifier, .. }
                if l.tpd.sym(fun).is_some_and(|s| l.u.symbol(s).name == "asInstanceOf") =>
            {
                reducible(l, qualifier)
            }
            _ => false,
        },
        Tree::Block { stats, expr } => match (l.tpd.items(stats), l.tpd.tree(expr)) {
            ([def], Tree::Closure { .. }) => matches!(l.tpd.tree(*def), Tree::DefDef { .. }),
            ([def], Tree::Typed { .. }) if one_method_class(l, *def) => true,
            _ => reducible(l, expr),
        },
        _ => false,
    }
}

/// Whether `def` defines a class whose body is one method.
fn one_method_class(l: &Lower, def: TNodeId) -> bool {
    let Tree::TypeDef { rhs, .. } = *l.tpd.tree(def) else { return false };
    let Tree::Template { body, .. } = *l.tpd.tree(rhs) else { return false };
    matches!(l.tpd.items(body), [m] if matches!(l.tpd.tree(*m), Tree::DefDef { .. }))
}
